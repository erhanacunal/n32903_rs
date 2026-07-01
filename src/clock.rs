//! System Clock / PLL / DDR configuration for N32903 (W55FA93)
//! Ported from wb_config.c and the N32903 NAND bootloader clock module.
//!
//! # Safety
//!
//! All functions in this module are `unsafe` because they manipulate
//! hardware registers directly. The clock-switch routine copies code
//! to SRAM and executes from there while DRAM is in self-refresh.

use crate::registers::*;

// ============================================================================
// Public Types
// ============================================================================

/// Clock source selection
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockSource {
    /// External crystal (12 MHz or 27 MHz depending on chip version)
    Ext = 0,
    /// 32 KHz RTC crystal
    X32k = 1,
    /// Audio PLL
    APll = 2,
    /// USB PLL
    UPll = 3,
}

/// Timer operating mode
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimerMode {
    OneShot = 0,
    Periodic = 1,
    Toggle = 2,
    Continuous = 3,
}

/// Complete clock configuration for `init_clock`.
#[derive(Clone, Copy, Debug)]
pub struct ClockConfig {
    /// Clock source
    pub src: ClockSource,
    /// PLL output frequency in KHz
    pub pll_khz: u32,
    /// System bus frequency in KHz
    pub sys_khz: u32,
    /// CPU core frequency in KHz
    pub cpu_khz: u32,
    /// AHB bus / HCLK frequency in KHz
    pub hclk_khz: u32,
    /// APB peripheral bus frequency in KHz
    pub apb_khz: u32,
}

impl ClockConfig {
    /// Default configuration matching the N32905 BSP reference:
    /// UPLL = 96 MHz, SYS = 96 MHz, CPU = 96 MHz, HCLK = 48 MHz, APB = 24 MHz
    pub const DEFAULT: Self = Self {
        src: ClockSource::UPll,
        pll_khz: 96_000,
        sys_khz: 96_000,
        cpu_khz: 96_000,
        hclk_khz: 48_000,
        apb_khz: 24_000,
    };
}

/// Snapshot of the current clock state
#[derive(Clone, Copy, Debug)]
pub struct ClockState {
    pub src: ClockSource,
    pub pll_khz: u32,
    pub sys_khz: u32,
    pub cpu_khz: u32,
    pub hclk_khz: u32,
    pub apb_khz: u32,
}

/// Clock configuration errors
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockError {
    /// PLL frequency must be >= system frequency
    PllTooLow,
    /// CPU frequency must be <= system frequency
    CpuTooHigh,
    /// HCLK frequency must be <= system frequency
    HclkTooHigh,
    /// APB frequency must be <= HCLK frequency
    ApbTooHigh,
    /// Invalid clock source for PLL mode
    InvalidSource,
    /// Could not find a valid PLL divider combination
    PllNoMatch,
}

// ============================================================================
// Static State
// ============================================================================

static mut EXT_CLOCK_KHZ: u32 = 27_000;
static mut SYS_CLK_SRC: ClockSource = ClockSource::Ext;
static mut UPLL_KHZ: u32 = 240_000;
static mut APLL_KHZ: u32 = 240_000;
static mut SYS_KHZ: u32 = 120_000;
static mut CPU_KHZ: u32 = 60_000;
static mut HCLK_KHZ: u32 = 60_000;
static mut APB_KHZ: u32 = 0;
static mut CPU_OVER_2X_HCLK: bool = false;

// ============================================================================
// Public API
// ============================================================================

/// Get the external crystal frequency in KHz (12 MHz or 27 MHz).
pub unsafe fn get_external_clock() -> u32 {
    if get_chip_version() == b'G' {
        EXT_CLOCK_KHZ = 12_000;
    } else {
        if reg_read32(REG_CHIPCFG) & 0xC == 0x8 {
            EXT_CLOCK_KHZ = 12_000;
        } else {
            EXT_CLOCK_KHZ = 27_000;
        }
    }
    EXT_CLOCK_KHZ
}

/// Get the chip version character: `b'G'` or `b'A'`.
pub unsafe fn get_chip_version() -> u8 {
    if reg_read32(0xFFFF_3EB4) == 0x5042_3238 {
        b'G'
    } else {
        b'A'
    }
}

/// Read the current clock state from the static globals populated
/// by the last `init_clock` / `sys_set_system_clock` call.
pub fn get_clock_state() -> ClockState {
    unsafe {
        ClockState {
            src: SYS_CLK_SRC,
            pll_khz: match SYS_CLK_SRC {
                ClockSource::UPll => UPLL_KHZ,
                ClockSource::APll => APLL_KHZ,
                _ => 0,
            },
            sys_khz: SYS_KHZ,
            cpu_khz: CPU_KHZ,
            hclk_khz: HCLK_KHZ,
            apb_khz: APB_KHZ,
        }
    }
}

/// Main clock initialisation entry point.
///
/// 1. Detects external crystal frequency
/// 2. Brings up DDR SDRAM
/// 3. Configures the PLL and switches the system clock
///
/// # Safety
///
/// Must be called early in the boot sequence.  Assumes the caller has
/// already set up exception vectors and stack pointers.
pub unsafe fn init_clock(config: ClockConfig) -> Result<(), ClockError> {
    // --- 1. Bring up DRAM (must happen before anything touches SDRAM) ---
    sys_init_ddr_start();

    let ext_freq = get_external_clock();

    reg_write32(REG_DQSODS, 0x1010);
    reg_write32(REG_CKDQSDS, 0x0088_8800); // E_CLKSKEW

    if ext_freq == 12_000 {
        reg_write32(REG_SDREF, 0x805A);
    } else {
        reg_write32(REG_SDREF, 0x80C0);
    }

    // DDR grade-6
    reg_write32(REG_SDTIME, 0x094E_7425);
    reg_write32(REG_SDMR, 0x22); // CAS Latency = 2

    sys_set_system_clock(
        config.src,
        config.pll_khz,
        config.sys_khz,
        config.cpu_khz,
        config.hclk_khz,
        config.apb_khz,
    )
}

/// Set the system clock to the requested frequencies.
pub unsafe fn sys_set_system_clock(
    src_clk: ClockSource,
    pll_khz: u32,
    sys_khz: u32,
    cpu_khz: u32,
    hclk_khz: u32,
    apb_khz: u32,
) -> Result<(), ClockError> {
    EXT_CLOCK_KHZ = get_external_clock();

    if src_clk != ClockSource::Ext {
        // Validate clock constraints
        if sys_khz > pll_khz {
            return Err(ClockError::PllTooLow);
        }
        if cpu_khz > sys_khz {
            return Err(ClockError::CpuTooHigh);
        }
        if hclk_khz > sys_khz {
            return Err(ClockError::HclkTooHigh);
        }
        if apb_khz > hclk_khz {
            return Err(ClockError::ApbTooHigh);
        }

        CPU_OVER_2X_HCLK = cpu_khz > hclk_khz * 2;

        let mut sys_div = pll_khz / sys_khz - 1;
        if pll_khz % sys_khz != 0 {
            sys_div += 1;
        }
        SYS_KHZ = pll_khz / (sys_div + 1);

        if CPU_OVER_2X_HCLK {
            HCLK_KHZ = SYS_KHZ / 6;

            let mut cpu_div = SYS_KHZ / cpu_khz - 1;
            if SYS_KHZ % cpu_khz != 0 {
                cpu_div += 1;
            }
            if cpu_div == 0 {
                cpu_div = 1;
            }
            if cpu_div > 5 {
                cpu_div = 5;
            }
            CPU_KHZ = SYS_KHZ / (cpu_div + 1);

            let hclk_div = 6u32;
            let apb_idx = SYS_KHZ / apb_khz;
            let mut apb_div = apb_idx / hclk_div;
            if apb_div > 0 {
                apb_div -= 1;
            }
            APB_KHZ = SYS_KHZ / ((apb_div + 1) * hclk_div);

            sys_clock_switch_start(src_clk, 0, HCLK_KHZ, sys_div, cpu_div, apb_div);
        } else {
            HCLK_KHZ = SYS_KHZ / 2;

            let mut cpu_div = SYS_KHZ / cpu_khz - 1;
            if SYS_KHZ % cpu_khz != 0 {
                cpu_div += 1;
            }
            if cpu_div > 1 {
                return Err(ClockError::CpuTooHigh);
            }
            CPU_KHZ = SYS_KHZ / (cpu_div + 1);

            let hclk_div = if cpu_div > 2 { cpu_div } else { 2u32 };
            let apb_idx = SYS_KHZ / apb_khz;
            let apb_div = apb_idx / hclk_div - 1;
            APB_KHZ = SYS_KHZ / ((apb_div + 1) * hclk_div);

            sys_clock_switch_start(src_clk, 0, HCLK_KHZ, sys_div, cpu_div, apb_div);
        }
    }

    // Configure PLL register
    let pll_reg = match src_clk {
        ClockSource::Ext => {
            SYS_CLK_SRC = ClockSource::Ext;
            return Ok(());
        }
        ClockSource::APll => {
            SYS_CLK_SRC = ClockSource::APll;
            APLL_KHZ = pll_khz;
            sys_get_pll_ctrl_reg(EXT_CLOCK_KHZ, APLL_KHZ)
        }
        ClockSource::UPll => {
            SYS_CLK_SRC = ClockSource::UPll;
            UPLL_KHZ = pll_khz;
            sys_get_pll_ctrl_reg(EXT_CLOCK_KHZ, UPLL_KHZ)
        }
        _ => return Err(ClockError::InvalidSource),
    };

    if pll_reg == 0 && src_clk != ClockSource::Ext {
        return Err(ClockError::PllNoMatch);
    }

    SYS_CLK_SRC = src_clk;
    sys_clock_switch_start(src_clk, pll_reg, HCLK_KHZ, 0, 0, 0);

    Ok(())
}

// ============================================================================
// PLL Computation
// ============================================================================

/// Get PLL output frequency in KHz for diagnostics
#[allow(dead_code)]
unsafe fn sys_get_pll_output_khz(sys_pll: ClockSource, fin_khz: u32) -> u32 {
    let au8_map: [u32; 4] = [1, 2, 2, 4];

    let pll_reg = match sys_pll {
        ClockSource::APll => reg_read32(REG_APLLCON),
        ClockSource::UPll => reg_read32(REG_UPLLCON),
        _ => return 0,
    };

    if pll_reg & PD != 0 {
        return 0; // PLL powered down
    }

    let nf = (pll_reg & FB_DV) + 2;
    let nr = ((pll_reg & IN_DV) >> 9) + 2;
    let no = au8_map[((pll_reg & OUT_DV) >> 14) as usize];

    fin_khz * nf / nr / no
}

/// Compute PLL control register value for a target frequency.
/// Iterates through valid NR/NF/NO combinations.
unsafe fn sys_get_pll_ctrl_reg(fin_khz: u32, target_khz: u32) -> u32 {
    let au32_array: [u32; 4] = [1, 2, 2, 4];
    let mut target = target_khz;

    loop {
        for out_dv in 0u32..4 {
            for in_dv in 0u32..32 {
                let nr = 2 * (in_dv + 2);
                // Input ref clock must be between 1 MHz and 15 MHz
                if fin_khz / nr < 1000 || fin_khz / nr > 15000 {
                    continue;
                }
                for fb_dv in 0u32..512 {
                    let nf = 2 * (fb_dv + 2);
                    let no = au32_array[out_dv as usize];

                    let vco = fin_khz * nf / nr;
                    // VCO must be between 100 MHz and 500 MHz
                    if vco < 100_000 || vco > 500_000 {
                        continue;
                    }

                    let result = fin_khz * nf / nr / no;
                    if target == result {
                        return (out_dv << 14) | (in_dv << 9) | fb_dv;
                    }
                }
            }
        }
        // Relax target by 4 MHz steps if no exact match
        if target < 4000 {
            break;
        }
        target -= 4000;
    }
    0
}

// ============================================================================
// DDR SDRAM Initialization
// ============================================================================

/// DDR controller init sequence.  Must execute from SRAM because DRAM
/// has not been initialized yet.
unsafe fn sys_init_ddr() {
    // Configure SDRAM controller
    reg_write32(0xB000_0224, 0x0000_0E6E);
    reg_write32(0xB000_0220, 0x1008_CE6E);
    reg_write32(0xB000_020C, 0x0000_0019);
    // DQS output delay
    reg_write32(0xB000_3030, 0x0000_1010);
    // Precharge all banks
    reg_write32(0xB000_3010, 0x0000_0005);
    reg_write32(0xB000_3004, 0x0000_0021);
    reg_write32(0xB000_3004, 0x0000_0023);
    reg_write32(0xB000_3004, 0x0000_0027);
    while reg_read32(0xB000_3004) & 0x4 != 0 {}
    // Mode register set
    reg_write32(0xB000_301C, 0x0000_1002);
    reg_write32(0xB000_3018, 0x0000_0122);
    reg_write32(0xB000_3004, 0x0000_0027);
    while reg_read32(0xB000_3004) & 0x4 != 0 {}
    // Extended mode register set
    reg_write32(0xB000_3004, 0x0000_002B);
    while reg_read32(0xB000_3004) & 0x8 != 0 {}
    reg_write32(0xB000_3004, 0x0000_002B);
    while reg_read32(0xB000_3004) & 0x8 != 0 {}
    reg_write32(0xB000_3018, 0x0000_0022);
    // Wait 250 cycles
    let mut delay: u32 = 250;
    while delay != 0 {
        delay -= 1;
    }
    // Normal operation mode
    reg_write32(0xB000_3004, 0x0000_0020);
    // SDRAM size / refresh
    reg_write32(0xB000_3034, 0x00AA_AA00);
    reg_write32(0xB000_3008, 0x0000_80C0);
    reg_write32(0xB000_00A0, 0x0000_0000);
}

// ============================================================================
// SRAM Execution Helpers
// ============================================================================

const PD_RAM_BASE: u32 = 0xFF00_0000;
const PD_RAM_START: u32 = 0xFF00_1000;
const PD_RAM_SIZE: usize = 0x2000;

/// Backup buffer for SRAM contents while DDR-init or clock-switch code runs there.
static mut TMP_BUF: [u8; PD_RAM_SIZE] = [0u8; PD_RAM_SIZE];

/// Copy `sys_init_ddr` into SRAM and run it there.
unsafe fn sys_init_ddr_start() {
    let aic_status = reg_read32(REG_AIC_IMR);
    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFF);

    let vram_base = PD_RAM_BASE as *mut u8;

    // 1. Back up current SRAM contents
    core::ptr::copy_nonoverlapping(vram_base, TMP_BUF.as_mut_ptr(), PD_RAM_SIZE);

    // 2. Copy sys_init_ddr body to SRAM
    core::ptr::copy_nonoverlapping(
        sys_init_ddr as *const u8,
        vram_base,
        PD_RAM_SIZE,
    );

    // 3. Jump to the copy in SRAM
    let func: unsafe extern "C" fn() = core::mem::transmute(vram_base as *const ());
    func();

    // 4. Restore original SRAM
    core::ptr::copy_nonoverlapping(TMP_BUF.as_ptr(), vram_base, PD_RAM_SIZE);

    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFF);
    reg_write32(REG_AIC_MECR, aic_status);
}

/// Clock switch routine - copied to SRAM and executed there.
unsafe fn sys_clock_switch(
    src_clk: ClockSource,
    pll_reg: u32,
    hclk_khz: u32,
    sys_div: u32,
    cpu_div: u32,
    apb_div: u32,
) {
    let u32_int_tmp = reg_read32(REG_AIC_IMR);
    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFE);

    // Adjust DDR low-freq mode
    if reg_read32(REG_CHIPCFG) & SDRAMSEL == 0x20 {
        // DDR2
        reg_write32(REG_SDEMR, reg_read32(REG_SDEMR) | DLLEN);
        if hclk_khz < 96_000 {
            reg_write32(REG_SDOPM, reg_read32(REG_SDOPM) | LOWFREQ);
        } else {
            reg_write32(REG_SDOPM, reg_read32(REG_SDOPM) & !LOWFREQ);
        }
    }

    // Delay
    let mut delay: u32 = 100;
    while delay != 0 {
        delay -= 1;
    }

    // Enter self-refresh
    reg_write32(REG_SDCMD, reg_read32(REG_SDCMD) | AUTOEXSELFREF | REF_CMD);

    match src_clk {
        ClockSource::Ext => {
            reg_write32(REG_CLKDIV0, (reg_read32(REG_CLKDIV0) & !SYSTEM_N0) | sys_div);
            if reg_read32(REG_CHIPCFG) & SDRAMSEL != 0 {
                reg_write32(REG_SDEMR, reg_read32(REG_SDEMR) | DLLEN);
                reg_write32(REG_SDOPM, reg_read32(REG_SDOPM) | LOWFREQ);
            }
            reg_write32(REG_CLKDIV0, reg_read32(REG_CLKDIV0) & !0xFF);
        }
        ClockSource::UPll => {
            reg_write32(REG_CLKDIV0, reg_read32(REG_CLKDIV0) | 0x02);
            reg_write32(REG_UPLLCON, pll_reg);
            // Delay
            let mut d: u32 = 1000;
            while d != 0 {
                d -= 1;
            }
            reg_write32(
                REG_CLKDIV0,
                (reg_read32(REG_CLKDIV0) & !0xF1F) | ((3u32 << 3) | sys_div),
            );
            reg_write32(
                REG_CLKDIV4,
                (reg_read32(REG_CLKDIV4) & !0xF0F) | (cpu_div | (apb_div << 8)),
            );
            // Delay
            let mut d2: u32 = 1000;
            while d2 != 0 {
                d2 -= 1;
            }
        }
        _ => {}
    }

    // Delay
    let mut d3: u32 = 1000;
    while d3 != 0 {
        d3 -= 1;
    }

    // Exit self-refresh
    reg_write32(REG_SDCMD, reg_read32(REG_SDCMD) & !REF_CMD);

    // Delay
    let mut d4: u32 = 1000;
    while d4 != 0 {
        d4 -= 1;
    }

    // Restore interrupt mask
    reg_write32(REG_AIC_MECR, u32_int_tmp);
}

/// Copy clock switch code to SRAM and execute.
unsafe fn sys_clock_switch_start(
    src_clk: ClockSource,
    pll_reg: u32,
    hclk: u32,
    sys_div: u32,
    cpu_div: u32,
    apb_div: u32,
) {
    let aic_status = reg_read32(REG_AIC_IMR);
    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFF);

    let vram_base = PD_RAM_BASE as u32;
    // Backup VRAM
    core::ptr::copy_nonoverlapping(
        (vram_base | 0x8000_0000) as *const u8,
        TMP_BUF.as_mut_ptr(),
        PD_RAM_SIZE,
    );
    // Copy sys_clock_switch to RAM
    core::ptr::copy_nonoverlapping(
        (sys_clock_switch as *const u8)
            .offset(-((PD_RAM_START - PD_RAM_BASE) as isize)),
        (vram_base | 0x8000_0000) as *mut u8,
        PD_RAM_SIZE,
    );

    let wb_func: unsafe extern "C" fn(ClockSource, u32, u32, u32, u32, u32) =
        core::mem::transmute(PD_RAM_START as *const ());
    wb_func(src_clk, pll_reg, hclk, sys_div, cpu_div, apb_div);

    // Restore VRAM
    core::ptr::copy_nonoverlapping(
        TMP_BUF.as_ptr(),
        (vram_base | 0x8000_0000) as *mut u8,
        PD_RAM_SIZE,
    );

    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFF);
    reg_write32(REG_AIC_MECR, aic_status);
}

// ============================================================================
// Extended Clock Functions (from wb_config.c)
// ============================================================================

/// Switch the system clock to the external crystal.
/// Also clears the UP2HCLK3X flag and CPU divider.
pub unsafe fn switch_to_external_clock() {
    reg_write32(REG_PWRCON, reg_read32(REG_PWRCON) & !UP2HCLK3X);
    reg_write32(
        REG_CLKDIV0,
        reg_read32(REG_CLKDIV0) & !(SYSTEM_N1 | SYSTEM_S | SYSTEM_N0),
    );
    reg_write32(REG_CLKDIV4, reg_read32(REG_CLKDIV4) & !CPU_N);
}

/// Set a PLL to a target frequency (KHz).
///
/// If the PLL is the current system clock source, returns its current
/// output frequency without modification.  Otherwise computes and writes
/// new PLL parameters.
///
/// Returns the resulting PLL output frequency in KHz, or 0 on error.
pub unsafe fn set_pll_clock(src: ClockSource, target_khz: u32) -> u32 {
    let fin_khz = get_external_clock();

    // If this PLL is already the system clock source, just read back
    let sys_src = (reg_read32(REG_CLKDIV0) & SYSTEM_S) >> 3;
    if (src == ClockSource::UPll && sys_src == 3)
        || (src == ClockSource::APll && sys_src == 2)
    {
        return sys_get_pll_output_khz(src, fin_khz);
    }

    // Compute and write new PLL value
    let pll_reg = sys_get_pll_ctrl_reg(fin_khz, target_khz);
    match src {
        ClockSource::APll => reg_write32(REG_APLLCON, pll_reg),
        ClockSource::UPll => reg_write32(REG_UPLLCON, pll_reg),
        _ => return 0,
    }

    sys_get_pll_output_khz(src, fin_khz)
}

/// Power down (or power up) a PLL.
///
/// Refuses to power down a PLL that is the current system clock source.
pub unsafe fn power_down_pll(src: ClockSource, power_down: bool) -> Result<(), ()> {
    if power_down {
        let sys_src = (reg_read32(REG_CLKDIV0) & SYSTEM_S) >> 3;
        if (src == ClockSource::UPll && sys_src == 3)
            || (src == ClockSource::APll && sys_src == 2)
        {
            return Err(());
        }
    }

    match src {
        ClockSource::APll => {
            if power_down {
                reg_write32(REG_APLLCON, reg_read32(REG_APLLCON) | PD);
            } else {
                reg_write32(REG_APLLCON, reg_read32(REG_APLLCON) & !PD);
            }
        }
        ClockSource::UPll => {
            if power_down {
                reg_write32(REG_UPLLCON, reg_read32(REG_UPLLCON) | PD);
            } else {
                reg_write32(REG_UPLLCON, reg_read32(REG_UPLLCON) & !PD);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Get the current CPU clock frequency in KHz.
pub unsafe fn get_cpu_clock() -> u32 {
    let fin_khz = get_external_clock();
    let sys_clk = reg_read32(REG_CLKDIV0);
    let sys_src = (sys_clk & SYSTEM_S) >> 3;

    let sys_clock = match sys_src {
        0 => fin_khz,
        2 => sys_get_pll_output_khz(ClockSource::APll, fin_khz),
        3 => sys_get_pll_output_khz(ClockSource::UPll, fin_khz),
        _ => fin_khz,
    };
    let sys_div = (sys_clk & SYSTEM_N0) + 1;
    let sys_freq = sys_clock / sys_div;

    sys_freq / ((reg_read32(REG_CLKDIV4) & CPU_N) + 1)
}

/// Set the CPU clock frequency in KHz (dynamic, no DRAM self-refresh).
///
/// Returns the new CPU clock frequency.
pub unsafe fn set_cpu_clock(target_khz: u32) -> u32 {
    let sys_freq = {
        let fin_khz = get_external_clock();
        let sys_clk = reg_read32(REG_CLKDIV0);
        let sys_src = (sys_clk & SYSTEM_S) >> 3;
        match sys_src {
            0 => fin_khz,
            2 => sys_get_pll_output_khz(ClockSource::APll, fin_khz),
            3 => sys_get_pll_output_khz(ClockSource::UPll, fin_khz),
            _ => fin_khz,
        }
    };

    let sys_freq = sys_freq / ((reg_read32(REG_CLKDIV0) & SYSTEM_N0) + 1);

    if sys_freq / target_khz == 1 {
        reg_write32(REG_CLKDIV4, reg_read32(REG_CLKDIV4) & !CPU_N);
    } else if sys_freq / target_khz == 2 {
        reg_write32(REG_CLKDIV4, (reg_read32(REG_CLKDIV4) & !CPU_N) | 0x01);
    }

    get_cpu_clock()
}

/// Get the current APB clock frequency in KHz.
pub unsafe fn get_apb_clock() -> u32 {
    let cpu_khz = get_cpu_clock();
    let hclk1 = if reg_read32(REG_CLKDIV4) & CPU_N == 0 {
        cpu_khz / 2
    } else {
        cpu_khz
    };
    hclk1 / (((reg_read32(REG_CLKDIV4) & APB_N) >> 8) + 1)
}

/// Set the APB clock frequency in KHz.
///
/// Returns the new APB clock frequency.
pub unsafe fn set_apb_clock(target_khz: u32) -> u32 {
    let cpu_khz = get_cpu_clock();
    let hclk1 = if reg_read32(REG_CLKDIV4) & CPU_N == 0 {
        cpu_khz / 2
    } else {
        cpu_khz
    };

    let mut apb_div = if hclk1 % target_khz != 0 {
        hclk1 / target_khz
    } else if hclk1 / target_khz != 0 {
        hclk1 / target_khz - 1
    } else {
        0
    };

    if apb_div > 15 {
        apb_div = 15;
    }

    reg_write32(
        REG_CLKDIV4,
        (reg_read32(REG_CLKDIV4) & !APB_N) | (apb_div << 8),
    );

    get_apb_clock()
}

/// Extended clock source enumeration for `get_clock`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockNode {
    UPll,
    APll,
    System,
    Hclk1,
    Hclk234,
    Pclk,
    Cpu,
}

/// Get the frequency (KHz) of any clock tree node.
pub unsafe fn get_clock(node: ClockNode) -> u32 {
    let fin_khz = get_external_clock();

    match node {
        ClockNode::UPll => sys_get_pll_output_khz(ClockSource::UPll, fin_khz),
        ClockNode::APll => sys_get_pll_output_khz(ClockSource::APll, fin_khz),
        ClockNode::System => {
            let reg = reg_read32(REG_CLKDIV0);
            let src_freq = match (reg & SYSTEM_S) >> 3 {
                0 => fin_khz,
                1 => 32,
                2 => sys_get_pll_output_khz(ClockSource::APll, fin_khz),
                3 => sys_get_pll_output_khz(ClockSource::UPll, fin_khz),
                _ => fin_khz,
            };
            src_freq / ((reg & SYSTEM_N0) + 1)
        }
        ClockNode::Hclk1 => get_clock(ClockNode::System) / 2,
        ClockNode::Hclk234 => get_clock(ClockNode::Hclk1),
        ClockNode::Pclk => get_apb_clock(),
        ClockNode::Cpu => get_cpu_clock(),
    }
}
