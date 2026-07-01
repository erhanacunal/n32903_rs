//! Register definitions for Nuvoton N32903 (W55FA93)
//! Ported from W55FA93_reg.h and wblib.h
//!
//! All register addresses and bitfield constants for the N3290x series SoC.
//! Uses volatile memory-mapped I/O via `core::ptr::read_volatile` /
//! `core::ptr::write_volatile`.

#![allow(dead_code)]

// ============================================================================
// Base Addresses
// ============================================================================
pub const SYS_BA: u32 = 0xB000_0000; // System Manager Control
pub const GCR_BA: u32 = SYS_BA;
pub const CLK_BA: u32 = 0xB000_0200; // Clock Controller
pub const SDRAM_BA: u32 = 0xB000_3000; // SDRAM Interface
pub const EDMA_BA: u32 = 0xB000_8000; // EDMA Controller
pub const SPU_BA: u32 = 0xB100_0000; // SPU Controller
pub const I2S_BA: u32 = 0xB100_1000; // I2S Controller
pub const VPOST_BA: u32 = 0xB100_2000; // VPOST Controller
pub const VIN_BA: u32 = 0xB100_3000; // Video-In Controller
pub const DMAC_BA: u32 = 0xB100_6000; // DMA Control
pub const FMI_BA: u32 = 0xB100_6800; // Flash Memory Interface
pub const USBD_BA: u32 = 0xB100_8000; // USB Device Control
pub const USBH_BA: u32 = 0xB100_9000; // USB Host Control
pub const JPG_BA: u32 = 0xB100_A000; // JPEG Engine Control
pub const BLT_BA: u32 = 0xB100_D000; // BitBlt Engine Control
pub const AIC_BA: u32 = 0xB800_0000; // Interrupt Controller
pub const GPIO_BA: u32 = 0xB800_1000; // GPIO Control
pub const TMR_BA: u32 = 0xB800_2000; // Timer Control
pub const RTC_BA: u32 = 0xB800_3000; // Real Time Clock Control
pub const I2C_BA: u32 = 0xB800_4000; // I2C Control
pub const KPI_BA: u32 = 0xB800_5000; // KPI Control
pub const PWM_BA: u32 = 0xB800_7000; // PWM Control
pub const UART_BA: u32 = 0xB800_8000; // UART Control
pub const SPI0_BA: u32 = 0xB800_C000; // SPI0 Control
pub const SPI1_BA: u32 = 0xB800_C400; // SPI1 Control
pub const ADC_BA: u32 = 0xB800_E000; // ADC Control

// ============================================================================
// System Manager (GCR) Registers
// ============================================================================
pub const REG_CHIPID: u32 = GCR_BA + 0x00;
pub const REG_CHIPCFG: u32 = GCR_BA + 0x04;
pub const REG_AHBCTL: u32 = GCR_BA + 0x10;
pub const REG_AHBIPRST: u32 = GCR_BA + 0x14;
pub const REG_APBIPRST: u32 = GCR_BA + 0x18;
pub const REG_MISCR: u32 = GCR_BA + 0x20;
pub const REG_GPAFUN: u32 = GCR_BA + 0x80;
pub const REG_GPBFUN: u32 = GCR_BA + 0x84;
pub const REG_GPCFUN: u32 = GCR_BA + 0x88;
pub const REG_GPDFUN: u32 = GCR_BA + 0x8C;
pub const REG_GPEFUN: u32 = GCR_BA + 0x90;
pub const REG_MISCPCR: u32 = GCR_BA + 0xA0;

// CHIPCFG bit fields
pub const SDRAMSEL: u32 = 0x30;
// CHIPCFG CLK_SRC bit
pub const CLK_SRC: u32 = 0x40;

// APBIPRST bit fields
pub const TMR1RST: u32 = 0x0000_0020; // Timer 1 Reset
pub const TMR0RST: u32 = 0x0000_0010; // Timer 0 Reset
pub const I2CRST: u32 = 0x0000_0100;  // I2C Reset
pub const PWMRST: u32 = 0x0000_0400;  // PWM Reset

// ============================================================================
// Clock Controller Registers
// ============================================================================
pub const REG_PWRCON: u32 = CLK_BA + 0x00;
pub const REG_AHBCLK: u32 = CLK_BA + 0x04;
pub const REG_APBCLK: u32 = CLK_BA + 0x08;
pub const REG_CLKDIV0: u32 = CLK_BA + 0x0C;
pub const REG_CLKDIV1: u32 = CLK_BA + 0x10;
pub const REG_CLKDIV2: u32 = CLK_BA + 0x14;
pub const REG_CLKDIV3: u32 = CLK_BA + 0x18;
pub const REG_CLKDIV4: u32 = CLK_BA + 0x1C;
pub const REG_APLLCON: u32 = CLK_BA + 0x20;
pub const REG_UPLLCON: u32 = CLK_BA + 0x24;

// PWRCON bit fields
pub const UP2HCLK3X: u32 = 0x20;

// AHBCLK bit fields
pub const ADO_CKE: u32 = 0x4000_0000;
pub const SPU_CKE: u32 = 0x0200_0000;
pub const HCLK4_CKE: u32 = 0x0100_0000;
pub const SD_CKE: u32 = 0x0080_0000;
pub const NAND_CKE: u32 = 0x0040_0000;
pub const SIC_CKE: u32 = 0x0020_0000;

// APBCLK bit fields
pub const TMR1_CKE: u32 = 0x0000_0200; // Timer1 Clock Enable
pub const TMR0_CKE: u32 = 0x0000_0100; // Timer0 Clock Enable
pub const UART1_CKE: u32 = 0x0000_0010; // UART1 Clock Enable
pub const UART0_CKE: u32 = 0x0000_0008; // UART0 Clock Enable
pub const PWM_CKE: u32 = 0x0000_0020;   // PWM Clock Enable
pub const I2C_CKE: u32 = 0x0000_0002;   // I2C Clock Enable
pub const RTC_CKE: u32 = 0x0000_0004;

// CLKDIV0 bit fields
pub const SYSTEM_N1: u32 = 0x0000_0F00;
pub const SYSTEM_S: u32 = 0x0000_0018;
pub const SYSTEM_N0: u32 = 0x0000_0007;

// CLKDIV3 bit fields (UART/ADC dividers)
pub const UART1_N: u32 = 0x0000_FF00;
pub const UART0_N: u32 = 0x0000_00FF;

// CLKDIV4 bit fields
pub const APB_N: u32 = 0x0000_0F00;
pub const CPU_N: u32 = 0x0000_000F;

// PLL bit fields
pub const PD: u32 = 0x0001_0000;
pub const BP: u32 = 0x0002_0000;
pub const OE: u32 = 0x0004_0000;
pub const OUT_DV: u32 = 0x0000_C000;
pub const IN_DV: u32 = 0x0000_3E00;
pub const FB_DV: u32 = 0x0000_01FF;

// ============================================================================
// SDRAM Controller Registers
// ============================================================================
pub const REG_SDOPM: u32 = SDRAM_BA + 0x00;
pub const REG_SDCMD: u32 = SDRAM_BA + 0x04;
pub const REG_SDREF: u32 = SDRAM_BA + 0x08;
pub const REG_SDSIZE0: u32 = SDRAM_BA + 0x10;
pub const REG_SDSIZE1: u32 = SDRAM_BA + 0x14;
pub const REG_SDMR: u32 = SDRAM_BA + 0x18;
pub const REG_SDEMR: u32 = SDRAM_BA + 0x1C;
pub const REG_SDEMR2: u32 = SDRAM_BA + 0x20;
pub const REG_SDEMR3: u32 = SDRAM_BA + 0x24;
pub const REG_SDTIME: u32 = SDRAM_BA + 0x28;
pub const REG_DQSODS: u32 = SDRAM_BA + 0x30;
pub const REG_CKDQSDS: u32 = SDRAM_BA + 0x34;
pub const REG_TESTCR: u32 = SDRAM_BA + 0x40;
pub const REG_TSTATUS: u32 = SDRAM_BA + 0x44;
pub const REG_TFDATA: u32 = SDRAM_BA + 0x48;
pub const REG_TGDATA: u32 = SDRAM_BA + 0x4C;

// SDCMD bit fields
pub const AUTOEXSELFREF: u32 = 0x20;
pub const REF_CMD: u32 = 0x08;
pub const SELF_REF: u32 = 0x10; // Self-Refresh Command
pub const CKE_H: u32 = 0x02;    // CKE High

// SDEMR bit fields
pub const DLLEN: u32 = 0x01;

// SDOPM bit fields
pub const LOWFREQ: u32 = 0x0004_0000;
pub const DRAM_EN: u32 = 0x0002;
pub const OPMODE: u32 = 0x0008; // Open Page Mode

// SDREF bit fields
pub const REF_EN: u32 = 0x8000;
pub const REFRATE: u32 = 0x7FFF;

// ============================================================================
// EBI (External Bus Interface) Registers
// ============================================================================
pub const REG_EXT0CON: u32 = GCR_BA + 0x28;
pub const REG_EXT1CON: u32 = GCR_BA + 0x2C;
pub const REG_EXT2CON: u32 = GCR_BA + 0x30;

// AHBCLK EBI bit
pub const EBI_CKE: u32 = 0x0000_0200; // EBI Clock Enable

// ============================================================================
// FMI (Flash Memory Interface) Registers
// ============================================================================
pub const REG_FMICR: u32 = FMI_BA + 0x000;
pub const REG_SMCSR: u32 = FMI_BA + 0x0A0;
pub const REG_SMTCR: u32 = FMI_BA + 0x0A4;
pub const REG_SMIER: u32 = FMI_BA + 0x0A8;
pub const REG_SMISR: u32 = FMI_BA + 0x0AC;
pub const REG_SMCMD: u32 = FMI_BA + 0x0B0;
pub const REG_SMADDR: u32 = FMI_BA + 0x0B4;
pub const REG_SMDATA: u32 = FMI_BA + 0x0B8;
pub const REG_SMREAREA_CTL: u32 = FMI_BA + 0x0BC;
pub const REG_SM_ECC_ST0: u32 = FMI_BA + 0x0D0;
pub const REG_SM_ECC_ST1: u32 = FMI_BA + 0x0D4;
pub const REG_SM_ECC_ST2: u32 = FMI_BA + 0x0D8;
pub const REG_SM_ECC_ST3: u32 = FMI_BA + 0x0DC;
pub const REG_BCH_ECC_ADDR0: u32 = FMI_BA + 0x100;
pub const REG_BCH_ECC_DATA0: u32 = FMI_BA + 0x160;
pub const REG_SMRA_0: u32 = FMI_BA + 0x200;

// FMICR
pub const FMI_SM_EN: u32 = 0x08;
pub const FMI_SWRST: u32 = 0x01;

// SMCSR bit fields
pub const SMCR_CS1: u32 = 0x0400_0000;
pub const SMCR_CS0: u32 = 0x0200_0000;
pub const SMCR_ECC_EN: u32 = 0x0080_0000;
pub const SMCR_BCH_TSEL: u32 = 0x0078_0000;
pub const SMCR_PSIZE: u32 = 0x0003_0000;
pub const SMCR_SRAM_INIT: u32 = 0x0000_0200;
pub const SMCR_ECC_3B_PROTECT: u32 = 0x0000_0100;
pub const SMCR_ECC_CHK: u32 = 0x0000_0080;
pub const SMCR_REDUN_AUTO_WEN: u32 = 0x0000_0010;
pub const SMCR_REDUN_REN: u32 = 0x0000_0008;
pub const SMCR_DRD_EN: u32 = 0x0000_0002;
pub const SMCR_SM_SWRST: u32 = 0x0000_0001;

// BCH T-sel values
pub const BCH_T15: u32 = 0x0040_0000;
pub const BCH_T12: u32 = 0x0020_0000;
pub const BCH_T8: u32 = 0x0010_0000;
pub const BCH_T4: u32 = 0x0008_0000;

// Page size values
pub const PSIZE_8K: u32 = 0x0003_0000;
pub const PSIZE_4K: u32 = 0x0002_0000;
pub const PSIZE_2K: u32 = 0x0001_0000;
// PSIZE_512 = 0

// SMISR bit fields
pub const SMISR_RB0: u32 = 0x0004_0000;
pub const SMISR_RB1: u32 = 0x0008_0000;
pub const SMISR_RB0_IF: u32 = 0x0000_0400;
pub const SMISR_RB1_IF: u32 = 0x0000_0800;
pub const SMISR_ECC_FIELD_IF: u32 = 0x0000_0004;
pub const SMISR_DMA_IF: u32 = 0x0000_0001;

// SMADDR
pub const EOA_SM: u32 = 0x8000_0000;

// SMREAREA_CTL bit fields
pub const SMRE_MECC: u32 = 0xFFFF_0000;
pub const SMRE_REA128_EXT: u32 = 0x0000_01FF;

// ============================================================================
// DMAC Registers
// ============================================================================
pub const REG_DMACCSR: u32 = DMAC_BA + 0x400;
pub const REG_DMACSAR: u32 = DMAC_BA + 0x408;
pub const REG_DMACBCR: u32 = DMAC_BA + 0x40C;

// DMACCSR bit fields
pub const FMI_BUSY: u32 = 0x0000_0200;
pub const DMAC_EN: u32 = 0x0000_0001;

// ============================================================================
// ============================================================================
// AIC (Advanced Interrupt Controller) Registers
// ============================================================================
pub const REG_AIC_SCR0: u32 = AIC_BA + 0x000;
pub const REG_AIC_SCR1: u32 = AIC_BA + 0x000; // aliased — first of SCR bank
pub const REG_AIC_IRSR: u32 = AIC_BA + 0x100; // Interrupt Raw Status
pub const REG_AIC_IASR: u32 = AIC_BA + 0x104; // Interrupt Active Status
pub const REG_AIC_ISR: u32 = AIC_BA + 0x108;  // Interrupt Status
pub const REG_AIC_IPER: u32 = AIC_BA + 0x10C; // Interrupt Priority Encoding
pub const REG_AIC_ISNR: u32 = AIC_BA + 0x110; // Interrupt Source Number
pub const REG_AIC_IMR: u32 = AIC_BA + 0x114;  // Interrupt Mask
pub const REG_AIC_OISR: u32 = AIC_BA + 0x118; // Output Interrupt Status
pub const REG_AIC_MECR: u32 = AIC_BA + 0x120; // Mask Enable Command
pub const REG_AIC_MDCR: u32 = AIC_BA + 0x124; // Mask Disable Command
pub const REG_AIC_SSCR: u32 = AIC_BA + 0x128; // Source Set Command
pub const REG_AIC_SCCR: u32 = AIC_BA + 0x12C; // Source Clear Command
pub const REG_AIC_EOSCR: u32 = AIC_BA + 0x130; // End of Service Command

// (REG_AIC_IMR previously defined at AIC_BA + 0x114)
// (REG_AIC_MECR/MDCR above override earlier definitions)
pub const REG_AIC_SCR2: u32 = AIC_BA + 0x008;
pub const REG_AIC_SCR3: u32 = AIC_BA + 0x00C;
pub const REG_AIC_SCR4: u32 = AIC_BA + 0x010;
pub const REG_AIC_SCR5: u32 = AIC_BA + 0x014;
pub const REG_AIC_SCR6: u32 = AIC_BA + 0x018;
pub const REG_AIC_SCR7: u32 = AIC_BA + 0x01C;
pub const REG_AIC_SCR8: u32 = AIC_BA + 0x020;
pub const REG_AIC_SCR9: u32 = AIC_BA + 0x024;
pub const REG_AIC_SCR10: u32 = AIC_BA + 0x028;
pub const REG_AIC_SCR11: u32 = AIC_BA + 0x02C;
pub const REG_AIC_SCR12: u32 = AIC_BA + 0x030;
pub const REG_AIC_SCR13: u32 = AIC_BA + 0x034;
pub const REG_AIC_SCR14: u32 = AIC_BA + 0x038;
pub const REG_AIC_SCR15: u32 = AIC_BA + 0x03C;
pub const REG_AIC_SCR16: u32 = AIC_BA + 0x040;
pub const REG_AIC_SCR17: u32 = AIC_BA + 0x044;
pub const REG_AIC_SCR18: u32 = AIC_BA + 0x048;
pub const REG_AIC_SCR19: u32 = AIC_BA + 0x04C;
pub const REG_AIC_SCR20: u32 = AIC_BA + 0x050;
pub const REG_AIC_SCR21: u32 = AIC_BA + 0x054;
pub const REG_AIC_SCR22: u32 = AIC_BA + 0x058;
pub const REG_AIC_SCR23: u32 = AIC_BA + 0x05C;
pub const REG_AIC_SCR24: u32 = AIC_BA + 0x060;
pub const REG_AIC_SCR25: u32 = AIC_BA + 0x064;
pub const REG_AIC_SCR26: u32 = AIC_BA + 0x068;
pub const REG_AIC_SCR27: u32 = AIC_BA + 0x06C;
pub const REG_AIC_SCR28: u32 = AIC_BA + 0x070;
pub const REG_AIC_SCR29: u32 = AIC_BA + 0x074;
pub const REG_AIC_SCR30: u32 = AIC_BA + 0x078;
pub const REG_AIC_SCR31: u32 = AIC_BA + 0x07C;

// ============================================================================
// IRQ Numbers (from wblib.h)
// ============================================================================
pub const IRQ_WDT: u32 = 1;
pub const IRQ_EXTINT0: u32 = 2;
pub const IRQ_EXTINT1: u32 = 3;
pub const IRQ_EXTINT2: u32 = 4;
pub const IRQ_EXTINT3: u32 = 5;
pub const IRQ_TMR0: u32 = 14;
pub const IRQ_TMR1: u32 = 15;
pub const IRQ_UART: u32 = 24;

// IRQ level constants
pub const IRQ_LEVEL_1: u32 = 1;
pub const IRQ_LEVEL_2: u32 = 2;
pub const IRQ_LEVEL_3: u32 = 3;
pub const IRQ_LEVEL_4: u32 = 4;
pub const IRQ_LEVEL_5: u32 = 5;
pub const IRQ_LEVEL_6: u32 = 6;
pub const IRQ_LEVEL_7: u32 = 7;

// ============================================================================
// UART Registers (UART_BA = 0xB800_8000)
// ============================================================================
pub const REG_UART0_RBR: u32 = UART_BA + 0x000;
pub const REG_UART0_THR: u32 = UART_BA + 0x000;
pub const REG_UART0_IER: u32 = UART_BA + 0x004;
pub const REG_UART0_FCR: u32 = UART_BA + 0x008;
pub const REG_UART0_LCR: u32 = UART_BA + 0x00C;
pub const REG_UART0_MCR: u32 = UART_BA + 0x010;
pub const REG_UART0_FSR: u32 = UART_BA + 0x018;
pub const REG_UART0_ISR: u32 = UART_BA + 0x01C;
pub const REG_UART0_TOR: u32 = UART_BA + 0x020;
pub const REG_UART0_BAUD: u32 = UART_BA + 0x024;

// UART1 registers (offset 0x100 from UART base)
pub const REG_UART1_RBR: u32 = UART_BA + 0x100;
pub const REG_UART1_THR: u32 = UART_BA + 0x100;
pub const REG_UART1_IER: u32 = UART_BA + 0x104;
pub const REG_UART1_FCR: u32 = UART_BA + 0x108;
pub const REG_UART1_LCR: u32 = UART_BA + 0x10C;
pub const REG_UART1_MCR: u32 = UART_BA + 0x110;
pub const REG_UART1_FSR: u32 = UART_BA + 0x118;
pub const REG_UART1_ISR: u32 = UART_BA + 0x11C;
pub const REG_UART1_TOR: u32 = UART_BA + 0x120;
pub const REG_UART1_BAUD: u32 = UART_BA + 0x124;

// Generic UART aliases (UART1, kept for backward compat)
pub const REG_UART_RBR: u32 = REG_UART1_RBR;
pub const REG_UART_THR: u32 = REG_UART1_THR;
pub const REG_UART_IER: u32 = REG_UART1_IER;
pub const REG_UART_FCR: u32 = REG_UART1_FCR;
pub const REG_UART_LCR: u32 = REG_UART1_LCR;
pub const REG_UART_MCR: u32 = REG_UART1_MCR;
pub const REG_UART_FSR: u32 = REG_UART1_FSR;
pub const REG_UART_ISR: u32 = REG_UART1_ISR;
pub const REG_UART_TOR: u32 = REG_UART1_TOR;
pub const REG_UART_BAUD: u32 = REG_UART1_BAUD;

// UART IER (Interrupt Enable Register) bit fields
pub const RDA_IEN: u32 = 0x0000_0001; // Receive Data Available Int Enable
pub const THRE_IEN: u32 = 0x0000_0002; // Transmit Holding Register Empty Int Enable
pub const RLS_IEN: u32 = 0x0000_0004; // Receive Line Status Int Enable
pub const RTO_IEN: u32 = 0x0000_0010; // RX Timeout Int Enable
pub const TIME_OUT_EN: u32 = 0x0000_0800; // Timeout counter enable
pub const AUTO_RTS_EN: u32 = 0x0000_1000; // Auto RTS flow control
pub const AUTO_CTS_EN: u32 = 0x0000_2000; // Auto CTS flow control
pub const EDMA_RX_EN: u32 = 0x0000_4000; // EDMA RX mode
pub const EDMA_TX_EN: u32 = 0x0000_8000; // EDMA TX mode

// UART ISR (Interrupt Status Register) bit fields
pub const RDA_IF: u32 = 0x0000_0001;
pub const THRE_IF: u32 = 0x0000_0002;
pub const RLS_IF: u32 = 0x0000_0004;
pub const MODEM_IF: u32 = 0x0000_0008;
pub const RTO_IF: u32 = 0x0000_0010;

// UART LCR (Line Control Register) bit fields
pub const WLS0: u32 = 0x0000_0001;
pub const WLS1: u32 = 0x0000_0002;
pub const NSB: u32 = 0x0000_0004; // Number of Stop Bits (0=1, 1=2)
pub const PBE: u32 = 0x0000_0008; // Parity Bit Enable
pub const EPE: u32 = 0x0000_0010; // Even Parity Enable
pub const BCB: u32 = 0x0000_0040; // Break Control Bit
pub const DLAB: u32 = 0x0000_0080; // Divisor Latch Access Bit

// Word length settings (WLS)
pub const WL_5BIT: u32 = 0x00;
pub const WL_6BIT: u32 = 0x01;
pub const WL_7BIT: u32 = 0x02;
pub const WL_8BIT: u32 = 0x03;

// UART FSR (FIFO Status Register) bit fields
pub const TX_EMPTY: u32 = 0x0040_0000;
pub const RX_NOT_EMPTY: u32 = 0x0000_0001;
pub const TX_FULL: u32 = 0x0080_0000;

// UART BAUD (Baud Rate Register) bit fields
pub const DIV_X_EN: u32 = 0x2000_0000;
pub const DIV_X_ONE: u32 = 0x1000_0000;
pub const DIVIDER_X: u32 = 0x0F00_0000;
pub const BAUD_RATE_DIVISOR: u32 = 0x0000_FFFF;

// UART FIFO trigger levels
pub const UART_FIFO_TRIG_1BYTE: u32 = 0x00;
pub const UART_FIFO_TRIG_4BYTE: u32 = 0x01;
pub const UART_FIFO_TRIG_8BYTE: u32 = 0x02;
pub const UART_FIFO_TRIG_14BYTE: u32 = 0x03;

// UART Multifunction pin settings
pub const MF_GPA10: u32 = 0x00A0_0000;
pub const MF_GPA11: u32 = 0x0080_0000;
pub const MF_GPB13: u32 = 0x0C00_0000; // I2C SDA
pub const MF_GPB14: u32 = 0x3000_0000; // I2C SCL
pub const MF_GPD1: u32 = 0x0000_000C;
pub const MF_GPD2: u32 = 0x0000_0030;

// ============================================================================
// GPIO Registers (GPIO_BA = 0xB800_1000)
// ============================================================================

// GPIO Port A (12-bit)
pub const REG_GPIOA_OMD: u32 = GPIO_BA + 0x0000; // Output Mode
pub const REG_GPIOA_PUEN: u32 = GPIO_BA + 0x0004; // Pull-Up Enable
pub const REG_GPIOA_DOUT: u32 = GPIO_BA + 0x0008; // Data Output Value
pub const REG_GPIOA_PIN: u32 = GPIO_BA + 0x000C; // Pin Input Value

// GPIO Port B (16-bit)
pub const REG_GPIOB_OMD: u32 = GPIO_BA + 0x0010;
pub const REG_GPIOB_PUEN: u32 = GPIO_BA + 0x0014;
pub const REG_GPIOB_DOUT: u32 = GPIO_BA + 0x0018;
pub const REG_GPIOB_PIN: u32 = GPIO_BA + 0x001C;

// GPIO Port C (16-bit)
pub const REG_GPIOC_OMD: u32 = GPIO_BA + 0x0020;
pub const REG_GPIOC_PUEN: u32 = GPIO_BA + 0x0024;
pub const REG_GPIOC_DOUT: u32 = GPIO_BA + 0x0028;
pub const REG_GPIOC_PIN: u32 = GPIO_BA + 0x002C;

// GPIO Port D (16-bit)
pub const REG_GPIOD_OMD: u32 = GPIO_BA + 0x0030;
pub const REG_GPIOD_PUEN: u32 = GPIO_BA + 0x0034;
pub const REG_GPIOD_DOUT: u32 = GPIO_BA + 0x0038;
pub const REG_GPIOD_PIN: u32 = GPIO_BA + 0x003C;

// GPIO Port E (12-bit)
pub const REG_GPIOE_OMD: u32 = GPIO_BA + 0x0040;
pub const REG_GPIOE_PUEN: u32 = GPIO_BA + 0x0044;
pub const REG_GPIOE_DOUT: u32 = GPIO_BA + 0x0048;
pub const REG_GPIOE_PIN: u32 = GPIO_BA + 0x004C;

// Debounce Control
pub const REG_DBNCECON: u32 = GPIO_BA + 0x0070;

// IRQ Source Group Registers (GPIO_BA + 0x80–0x90)
pub const REG_IRQSRCGPA: u32 = GPIO_BA + 0x0080;
pub const REG_IRQSRCGPB: u32 = GPIO_BA + 0x0084;
pub const REG_IRQSRCGPC: u32 = GPIO_BA + 0x0088;
pub const REG_IRQSRCGPD: u32 = GPIO_BA + 0x008C;
pub const REG_IRQSRCGPE: u32 = GPIO_BA + 0x0090;

// IRQ Enable Registers (GPIO_BA + 0xA0–0xB0)
pub const REG_IRQENGPA: u32 = GPIO_BA + 0x00A0;
pub const REG_IRQENGPB: u32 = GPIO_BA + 0x00A4;
pub const REG_IRQENGPC: u32 = GPIO_BA + 0x00A8;
pub const REG_IRQENGPD: u32 = GPIO_BA + 0x00AC;
pub const REG_IRQENGPE: u32 = GPIO_BA + 0x00B0;

// Latch Registers
pub const REG_IRQLHSEL: u32 = GPIO_BA + 0x00C0;

// Latched Interrupt Values (GPIO_BA + 0xD0–0xE0)
pub const REG_IRQLHGPA: u32 = GPIO_BA + 0x00D0;
pub const REG_IRQLHGPB: u32 = GPIO_BA + 0x00D4;
pub const REG_IRQLHGPC: u32 = GPIO_BA + 0x00D8;
pub const REG_IRQLHGPD: u32 = GPIO_BA + 0x00DC;
pub const REG_IRQLHGPE: u32 = GPIO_BA + 0x00E0;

// Trigger Source Registers
pub const REG_IRQTGSRC0: u32 = GPIO_BA + 0x00F0; // Port A [15:0], Port B [31:16]
pub const REG_IRQTGSRC1: u32 = GPIO_BA + 0x00F4; // Port C [15:0], Port D [31:16]
pub const REG_IRQTGSRC2: u32 = GPIO_BA + 0x00F8; // Port E [15:0]

// GPIO Port masks (pin counts)
pub const GPIO_PORTA_MASK: u16 = 0x0FFF; // 12 pins
pub const GPIO_PORTB_MASK: u16 = 0xFFFF; // 16 pins
pub const GPIO_PORTC_MASK: u16 = 0xFFFF; // 16 pins
pub const GPIO_PORTD_MASK: u16 = 0xFFFF; // 16 pins
pub const GPIO_PORTE_MASK: u16 = 0x0FFF; // 12 pins

// ============================================================================
// Timer Registers (TMR_BA = 0xB800_2000)
// ============================================================================
pub const REG_TCSR0: u32 = TMR_BA + 0x00; // Timer Control/Status Register 0
pub const REG_TCSR1: u32 = TMR_BA + 0x04; // Timer Control/Status Register 1
pub const REG_TICR0: u32 = TMR_BA + 0x08; // Timer Initial Count Register 0
pub const REG_TICR1: u32 = TMR_BA + 0x0C; // Timer Initial Count Register 1
pub const REG_TDR0: u32 = TMR_BA + 0x10; // Timer Data Register 0
pub const REG_TDR1: u32 = TMR_BA + 0x14; // Timer Data Register 1
pub const REG_TISR: u32 = TMR_BA + 0x18; // Timer Interrupt Status Register
pub const REG_WTCR: u32 = TMR_BA + 0x1C; // Watchdog Timer Control Register

// Timer Interrupt Status bits
pub const TIF0: u32 = 0x0000_0001; // Timer 0 Interrupt Flag
pub const TIF1: u32 = 0x0000_0002; // Timer 1 Interrupt Flag

// Timer Operating Modes (written to bits 31:29 of TCSR via ((mode | 0xC) << 27))
pub const TIMER_ONESHOT: u32 = 0x00;
pub const TIMER_PERIODIC: u32 = 0x01;
pub const TIMER_TOGGLE: u32 = 0x02;
pub const TIMER_CONTINUOUS: u32 = 0x03;

// TCSR bit field helper: bits 28:24 are prescaler (0-15), bit 12 is CEN (enable)
pub const TCSR_PRESCALE_POS: u32 = 24;
pub const TCSR_MODE_POS: u32 = 27;
pub const TCSR_CEN_MASK: u32 = 0x0000_0C00; // CEN + mode bits in the enable word
pub const TCSR_PRESCALE_MASK: u32 = 0x1F00_0000;

// Watchdog Timer Control (REG_WTCR) bits
pub const WDT_WTR: u32 = 0x01;   // Write Trigger (clear counter)
pub const WDT_WTE: u32 = 0x02;   // Watchdog Timer Enable
pub const WDT_WTRE: u32 = 0x04;  // Watchdog Timer Reset Enable
pub const WDT_WTIF: u32 = 0x08;  // Watchdog Timer Interrupt Flag
pub const WDT_WTIE: u32 = 0x40;  // Watchdog Timer Interrupt Enable
pub const WDT_WTEN: u32 = 0x80;  // Watchdog Timer System Enable
pub const WDT_INTERVAL_MASK: u32 = 0x30; // Interval selection (bits 5:4)

// Watchdog interval values
pub const WDT_INTERVAL_0_5S: u32 = 0x00; // 0.5 seconds
pub const WDT_INTERVAL_5S: u32 = 0x10;   // 5 seconds
pub const WDT_INTERVAL_10S: u32 = 0x20;  // 10 seconds
pub const WDT_INTERVAL_20S: u32 = 0x30;  // 20 seconds

// ============================================================================
// RTC Registers
// ============================================================================
pub const AER: u32 = RTC_BA + 0x04;
pub const PWRON: u32 = RTC_BA + 0x34;
pub const RIIR: u32 = RTC_BA + 0x2C;

// ============================================================================
// VPOST (Video Post-Processor / LCD Controller) Registers
// VPOST_BA = 0xB100_2000
// ============================================================================

// LCD Controller Control Register
pub const REG_LCM_LCDCCtl: u32 = VPOST_BA + 0x00;
    pub const LCDCCtl_FSADDR_SEL: u32 = 0x8000_0000; // Frame buffer address select
    pub const LCDCCtl_HAW_656: u32 = 0x4000_0000;     // CCIR656 horizontal active width
    pub const LCDCCtl_PRDB_SEL: u32 = 0x0030_0000;    // Parallel RGB data bus select (bits 21:20)
    pub const LCDCCtl_YUVBL: u32 = 0x0001_0000;       // YUV endian select
    pub const LCDCCtl_FBDS: u32 = 0x0000_000E;        // Frame Buffer Data Selection (bits 3:1)
    pub const LCDCCtl_LCDRUN: u32 = 0x0000_0001;      // LCD Controller Run

// LCD Controller Parameter Register
pub const REG_LCM_LCDCPrm: u32 = VPOST_BA + 0x04;
    pub const LCDCPrm_Even_Field_AL: u32 = 0xF000_0000;
    pub const LCDCPrm_Odd_Field_AL: u32 = 0x0F00_0000;
    pub const LCDCPrm_F1_EL: u32 = 0x00FF_8000;
    pub const LCDCPrm_LCDSynTv: u32 = 0x0000_0100;    // LCD timing sync with TV
    pub const LCDCPrm_SRGB_EL_SEL: u32 = 0x0000_00C0; // Even line color order
    pub const LCDCPrm_SRGB_OL_SEL: u32 = 0x0000_0030; // Odd line color order
    pub const LCDCPrm_LCDDataSel: u32 = 0x0000_000C;  // LCD data interface select
    pub const LCDCPrm_LCDTYPE: u32 = 0x0000_0003;     // LCD device type

// LCD Controller Interrupt Register
pub const REG_LCM_LCDCInt: u32 = VPOST_BA + 0x08;
    pub const LCDCInt_MPUCPLINTEN: u32 = 0x0010_0000;
    pub const LCDCInt_TVFIELDINTEN: u32 = 0x0004_0000;
    pub const LCDCInt_VINTEN: u32 = 0x0002_0000;
    pub const LCDCInt_HINTEN: u32 = 0x0001_0000;
    pub const LCDCInt_MPUCPL: u32 = 0x0000_0010;
    pub const LCDCInt_TVFIELDINT: u32 = 0x0000_0004;
    pub const LCDCInt_VINT: u32 = 0x0000_0002;
    pub const LCDCInt_HINT: u32 = 0x0000_0001;

// Reserved / Frame End Address
pub const REG_FEADDR: u32 = VPOST_BA + 0x0C;

// Timing Control Register 1 — Horizontal timing
pub const REG_LCM_TCON1: u32 = VPOST_BA + 0x10;
    pub const TCON1_HSPW: u32 = 0x00FF_0000; // HSYNC pulse width
    pub const TCON1_HBPD: u32 = 0x0000_FF00; // Horizontal back porch
    pub const TCON1_HFPD: u32 = 0x0000_00FF; // Horizontal front porch

// Timing Control Register 2 — Vertical timing
pub const REG_LCM_TCON2: u32 = VPOST_BA + 0x14;
    pub const TCON2_VSPW: u32 = 0x00FF_0000; // VSYNC pulse width
    pub const TCON2_VBPD: u32 = 0x0000_FF00; // Vertical back porch
    pub const TCON2_VFPD: u32 = 0x0000_00FF; // Vertical front porch

// Timing Control Register 3 — Resolution
pub const REG_LCM_TCON3: u32 = VPOST_BA + 0x18;
    pub const TCON3_PPL: u32 = 0xFFFF_0000; // Pixels Per Line (bits 31:16)
    pub const TCON3_LPP: u32 = 0x0000_FFFF; // Lines Per Panel (bits 15:0)

// Timing Control Register 4 — Polarity / extra timing
pub const REG_LCM_TCON4: u32 = VPOST_BA + 0x1C;
    pub const TCON4_TAPN: u32 = 0x07FF_0000; // Total Active Pixel Number
    pub const TCON4_MVPW: u32 = 0x0000_FF00;
    pub const TCON4_MPU_FMARKP: u32 = 0x0000_0020;
    pub const TCON4_MPU_VSYNCP: u32 = 0x0000_0010;
    pub const TCON4_VSP: u32 = 0x0000_0008;  // VSYNC polarity (1=active low)
    pub const TCON4_HSP: u32 = 0x0000_0004;  // HSYNC polarity (1=active low)
    pub const TCON4_DEP: u32 = 0x0000_0002;  // VDEN polarity (1=active low)
    pub const TCON4_PCLKP: u32 = 0x0000_0001; // Pixel clock polarity (1=rising edge)

// MPU Command Register
pub const REG_LCM_MPUCMD: u32 = VPOST_BA + 0x20;
    pub const MPUCMD_MPU_VFPIN_SEL: u32 = 0x8000_0000;
    pub const MPUCMD_DIS_SEL: u32 = 0x4000_0000;
    pub const MPUCMD_CMD_DISn: u32 = 0x2000_0000;
    pub const MPUCMD_MPU_CS: u32 = 0x1000_0000;
    pub const MPUCMD_MPU_ON: u32 = 0x0800_0000;
    pub const MPUCMD_BUSY: u32 = 0x0400_0000;
    pub const MPUCMD_WR_RS: u32 = 0x0200_0000;
    pub const MPUCMD_MPU_RWn: u32 = 0x0100_0000;
    pub const MPUCMD_MPU68: u32 = 0x0080_0000;
    pub const MPUCMD_FMARK: u32 = 0x0040_0000;
    pub const MPUCMD_MPU_SI_SEL: u32 = 0x000F_0000; // MPU bus mode
    pub const MPUCMD_MPU_CMD: u32 = 0x0000_FFFF;    // Command/data

// MPU Timing Setting
pub const REG_LCM_MPUTS: u32 = VPOST_BA + 0x24;
    pub const MPUTS_CSnF2DCt: u32 = 0xFF00_0000;
    pub const MPUTS_WRnR2CSnRt: u32 = 0x00FF_0000;
    pub const MPUTS_WRnLWt: u32 = 0x0000_FF00;
    pub const MPUTS_CSnF2WRnFt: u32 = 0x0000_00FF;

// OSD Control Register
pub const REG_LCM_OSD_CTL: u32 = VPOST_BA + 0x28;
    pub const OSD_CTL_OSD_EN: u32 = 0x8000_0000;
    pub const OSD_CTL_OSD_FSEL: u32 = 0x0F00_0000;
    pub const OSD_CTL_OSD_TC: u32 = 0x00FF_FFFF;

// OSD Picture Size
pub const REG_LCM_OSD_SIZE: u32 = VPOST_BA + 0x2C;
    pub const OSD_SIZE_OSD_VSIZE: u32 = 0x03FF_0000;
    pub const OSD_SIZE_OSD_HSIZE: u32 = 0x0000_03FF;

// OSD Start Position
pub const REG_LCM_OSD_SP: u32 = VPOST_BA + 0x30;
    pub const OSD_SP_OSD_SY: u32 = 0x03FF_0000;
    pub const OSD_SP_OSD_SX: u32 = 0x0000_03FF;

// OSD Bar End Position
pub const REG_LCM_OSD_BEP: u32 = VPOST_BA + 0x34;
    pub const OSD_BEP_OSD_1BEY: u32 = 0x03FF_0000;
    pub const OSD_BEP_OSD_1BEX: u32 = 0x0000_03FF;

// OSD Bar Offset
pub const REG_LCM_OSD_BO: u32 = VPOST_BA + 0x38;
    pub const OSD_BO_OSD_BOY: u32 = 0x03FF_0000;
    pub const OSD_BO_OSD_BOX: u32 = 0x0000_03FF;

// Color Bar Active Region
pub const REG_LCM_CBAR: u32 = VPOST_BA + 0x3C;
    pub const CBAR_CTL_EQ6SEL: u32 = 0x1000_0000;
    pub const CBAR_CTL_HCBEPC: u32 = 0x03FF_0000;
    pub const CBAR_CTL_HCBBPC: u32 = 0x0000_03FF;

// TV Control Register
pub const REG_LCM_TVCtl: u32 = VPOST_BA + 0x40;
    pub const TVCtl_TvField: u32 = 0x8000_0000;   // TV field status (RO)
    pub const TVCtl_TvCMM: u32 = 0x0001_0000;     // TV color modulation method
    pub const TVCtl_FBSIZE: u32 = 0x0000_C000;    // Frame buffer size
    pub const TVCtl_LCDSrc: u32 = 0x0000_0C00;    // LCD image source
    pub const TVCtl_TvSrc: u32 = 0x0000_0300;     // TV image source
    pub const TVCtl_TvLBSA: u32 = 0x0000_0040;     // TV line buffer scaling
    pub const TVCtl_NotchE: u32 = 0x0000_0020;     // Notch filter enable
    pub const TVCtl_Tvdac: u32 = 0x0000_0010;      // TV DAC enable
    pub const TVCtl_TvInter: u32 = 0x0000_0008;    // Interlace mode
    pub const TVCtl_TvSys: u32 = 0x0000_0004;      // TV system (0=NTSC, 1=PAL)
    pub const TVCtl_TvColor: u32 = 0x0000_0002;    // Color/B&W
    pub const TVCtl_TvSleep: u32 = 0x0000_0001;     // TV encoder enable

// IIR Filter Coefficients
pub const REG_LCM_IIRA: u32 = VPOST_BA + 0x44;
pub const REG_LCM_IIRB: u32 = VPOST_BA + 0x48;

// Background Color Setting
pub const REG_LCM_COLORSET: u32 = VPOST_BA + 0x4C;
    pub const COLORSET_Color_R: u32 = 0x00FF_0000;
    pub const COLORSET_Color_G: u32 = 0x0000_FF00;
    pub const COLORSET_Color_B: u32 = 0x0000_00FF;

// Frame Buffer Start Address
pub const REG_LCM_FSADDR: u32 = VPOST_BA + 0x50;

// TV Display Start Control
pub const REG_LCM_TVDisCtl: u32 = VPOST_BA + 0x54;
    pub const TVDisCtl_FFRHS: u32 = 0xFF00_0000;
    pub const TVDisCtl_LCDHB: u32 = 0x00FF_0000;
    pub const TVDisCtl_TVDVS: u32 = 0x0000_FF00;
    pub const TVDisCtl_TVDHS: u32 = 0x0000_00FF;

// Color Burst Amplitude Control
pub const REG_LCM_CBACtl: u32 = VPOST_BA + 0x58;
// TV Contrast
pub const REG_LCM_TVContrast: u32 = VPOST_BA + 0x64;
// TV Brightness
pub const REG_LCM_TVBright: u32 = VPOST_BA + 0x68;
// Line Stripe Offset
pub const REG_LCM_LINE_STRIPE: u32 = VPOST_BA + 0x70;
// OSD Frame Buffer Address
pub const REG_LCM_OSD_ADDR: u32 = VPOST_BA + 0x5C;
// Color space conversion test registers
pub const REG_LCM_RGBin: u32 = VPOST_BA + 0x74;
pub const REG_LCM_YCbCrout: u32 = VPOST_BA + 0x78;
pub const REG_LCM_YCbCrin: u32 = VPOST_BA + 0x7C;
pub const REG_LCM_RGBout: u32 = VPOST_BA + 0x80;

// CLKDIV1 VPOST clock bits
pub const VPOST_N1: u32 = 0x0000_FF00;
pub const VPOST_S: u32 = 0x0000_0018;
pub const VPOST_N0: u32 = 0x0000_0007;

// APBIPRST / AHBCLK VPOST bits
pub const VPOSTRST: u32 = 0x0000_0400; // VPOST Reset (in REG_APBIPRST)
pub const VPOST_CKE: u32 = 0x0800_0000; // VPOST Clock Enable (in REG_AHBCLK)

// IRQ number
pub const IRQ_VPOST: u32 = 31;

// ============================================================================
// SPU (Sound Processing Unit) Registers
// SPU_BA = 0xB100_0000
// ============================================================================

// SPU Control and Status
pub const REG_SPU_CTRL: u32 = SPU_BA + 0x00;
    pub const SPU_FIFO_SIZE: u32 = 0x0400_0000; // FIFO size = 4
    pub const SPU_SWRST: u32 = 0x0001_0000;     // SW reset
    pub const SPU_EN: u32 = 0x0000_0001;         // SPU enable

// DAC Parameter
pub const REG_SPU_DAC_PAR: u32 = SPU_BA + 0x04;
    pub const ZERO_EN: u32 = 0x0200_0000;     // Zero-cross detect
    pub const EQU_EN: u32 = 0x0100_0000;      // Equalizer enable
    pub const DISCHARGE_EN: u32 = 0x0000_4000; // Discharge path enable
    pub const DISCHARGE_CON: u32 = 0x0000_3000; // Discharge control
    pub const POP_CON: u32 = 0x0000_0030;      // Pop noise control

// DAC Volume
pub const REG_SPU_DAC_VOL: u32 = SPU_BA + 0x08;
    pub const DWA_SEL: u32 = 0xC000_0000;
    pub const ANA_PD: u32 = 0x03FF_0000;      // DAC power-down (bits 25:16)
    pub const LHPVL: u32 = 0x0000_1F00;       // Left headphone vol
    pub const RHPVL: u32 = 0x0000_001F;       // Right headphone vol

// Equalizer Gain 0 (bands 1-8)
pub const REG_SPU_EQGain0: u32 = SPU_BA + 0x0C;
    pub const GAIN08: u32 = 0xF000_0000;
    pub const GAIN07: u32 = 0x0F00_0000;
    pub const GAIN06: u32 = 0x00F0_0000;
    pub const GAIN05: u32 = 0x000F_0000;
    pub const GAIN04: u32 = 0x0000_F000;
    pub const GAIN03: u32 = 0x0000_0F00;
    pub const GAIN02: u32 = 0x0000_00F0;
    pub const GAIN01: u32 = 0x0000_000F;

// Equalizer Gain 1 (DC, bands 9-10)
pub const REG_SPU_EQGain1: u32 = SPU_BA + 0x10;
    pub const GAINDC: u32 = 0x000F_0000;
    pub const GAIN10: u32 = 0x0000_00F0;
    pub const GAIN09: u32 = 0x0000_000F;

// Channel Enable
pub const REG_SPU_CH_EN: u32 = SPU_BA + 0x14;

// Channel IRQ Flags
pub const REG_SPU_CH_IRQ: u32 = SPU_BA + 0x18;

// Channel Pause
pub const REG_SPU_CH_PAUSE: u32 = SPU_BA + 0x1C;

// Channel Control
pub const REG_SPU_CH_CTRL: u32 = SPU_BA + 0x20;
    pub const CH_NO: u32 = 0x1F00_0000;      // Channel number (bits 28:24)
    pub const CH_RST: u32 = 0x0000_0100;     // Channel reset
    pub const UP_IRQ: u32 = 0x0000_0080;     // Update IRQ partial
    pub const UP_DFA: u32 = 0x0000_0040;     // Update DFA partial
    pub const UP_PAN: u32 = 0x0000_0020;     // Update PAN partial
    pub const UP_VOL: u32 = 0x0000_0010;     // Update Volume partial
    pub const UP_PAUSE_ADDR: u32 = 0x0000_0008; // Update Pause addr
    pub const FN_IRQ_EN: u32 = 0x0000_0004;  // Function-done IRQ enable
    pub const CH_FN: u32 = 0x0000_0003;      // Channel function (bits 1:0)
    // CH_CTRL commands
    pub const CH_CTRL_LOAD: u32 = 0x01;       // Load selected channel
    pub const CH_CTRL_UPDATE_ALL: u32 = 0x02; // Update all settings
    pub const CH_CTRL_UPDATE_PARTIAL: u32 = 0x03; // Update partial
    pub const CH_CTRL_UPDATE_ALL_PARTIALS: u32 = 0xF0; // All partials mask

// Per-channel registers (shared — selected via CH_NO in CH_CTRL)
pub const REG_SPU_S_ADDR: u32 = SPU_BA + 0x24;   // Source start (base) address
pub const REG_SPU_M_ADDR: u32 = SPU_BA + 0x28;   // Threshold address
pub const REG_SPU_E_ADDR: u32 = SPU_BA + 0x2C;   // End address
pub const REG_SPU_TONE_PULSE: u32 = SPU_BA + 0x28; // Tone pulse (shares M_ADDR)
pub const REG_SPU_TONE_AMP: u32 = SPU_BA + 0x2C;   // Tone amplitude (shares E_ADDR)
pub const REG_SPU_CH_PAR_1: u32 = SPU_BA + 0x30;   // Channel param 1
    pub const CH_VOL: u32 = 0x7F00_0000;  // Volume (bits 30:24)
    pub const PAN_L: u32 = 0x001F_0000;   // Left PAN (bits 20:16)
    pub const PAN_R: u32 = 0x0000_1F00;   // Right PAN (bits 12:8)
    pub const SRC_TYPE: u32 = 0x0000_0007; // Source type (bits 2:0)
pub const REG_SPU_CH_PAR_2: u32 = SPU_BA + 0x34;   // Channel param 2
    pub const DFA: u32 = 0x0000_1FFF;     // DFA (bits 12:0)

// Channel Event
pub const REG_SPU_CH_EVENT: u32 = SPU_BA + 0x38;
    pub const SUB_IDX: u32 = 0x3F00_0000;    // Sub-index
    pub const EVENT_IDX: u32 = 0x00FF_0000;  // Event index
    pub const EV_USR_FG: u32 = 0x0000_2000;  // User event flag
    pub const EV_SLN_FG: u32 = 0x0000_1000;  // Silent event flag
    pub const EV_LP_FG: u32 = 0x0000_0800;   // Loop start flag
    pub const EV_END_FG: u32 = 0x0000_0400;  // End event flag
    pub const END_FG: u32 = 0x0000_0200;     // End address flag
    pub const TH_FG: u32 = 0x0000_0100;      // Threshold address flag
    pub const AT_CLR_EN: u32 = 0x0000_0080;  // Auto-clear IRQ after read
    pub const EV_USR_EN: u32 = 0x0000_0020;  // User event enable
    pub const EV_SLN_EN: u32 = 0x0000_0010;  // Silent event enable
    pub const EV_LP_EN: u32 = 0x0000_0008;   // Loop start enable
    pub const EV_END_EN: u32 = 0x0000_0004;  // End event enable
    pub const END_EN: u32 = 0x0000_0002;     // End address enable
    pub const TH_EN: u32 = 0x0000_0001;      // Threshold address enable

// Misc
pub const REG_SPU_CUR_ADDR: u32 = SPU_BA + 0x40;  // Current address
pub const REG_SPU_LP_ADDR: u32 = SPU_BA + 0x44;   // Loop start address
pub const REG_SPU_PA_ADDR: u32 = SPU_BA + 0x44;   // Pause address
pub const REG_SPU_P_BYTES: u32 = SPU_BA + 0x48;   // Loop play byte count

// Source type values
pub const SPU_SRC_MDPCM: u32 = 0x00;
pub const SPU_SRC_LP8: u32 = 0x01;
pub const SPU_SRC_PCM16: u32 = 0x03;
pub const SPU_SRC_TONE: u32 = 0x04;
pub const SPU_SRC_PCM16_MONO: u32 = 0x05;
pub const SPU_SRC_PCM16_STEREO_L: u32 = 0x06;
pub const SPU_SRC_PCM16_STEREO_R: u32 = 0x07;

// IRQ number
pub const IRQ_SPU: u32 = 6;

// ============================================================================
// Constants
// ============================================================================
pub const EXTERNAL_CRYSTAL_CLOCK: u32 = 12_000_000; // 12 MHz default

// ============================================================================
// PWM (Pulse Width Modulation) Registers
// PWM_BA = 0xB800_7000
// ============================================================================

// PWM Prescaler Register
pub const REG_PPR: u32 = PWM_BA + 0x000;
    pub const DZI1: u32 = 0xFF00_0000; // Dead zone interval 1 (bits 31:24)
    pub const DZI0: u32 = 0x00FF_0000; // Dead zone interval 0 (bits 23:16)
    pub const CP1: u32 = 0x0000_FF00;  // Clock prescaler 1 for ch2/ch3
    pub const CP0: u32 = 0x0000_00FF;  // Clock prescaler 0 for ch0/ch1

// PWM Clock Select Register
pub const REG_PWM_CSR: u32 = PWM_BA + 0x004;
    pub const CSR3: u32 = 0x0000_7000; // ch3 clock select (bits 14:12)
    pub const CSR2: u32 = 0x0000_0700; // ch2 clock select (bits 10:8)
    pub const CSR1: u32 = 0x0000_0070; // ch1 clock select (bits 6:4)
    pub const CSR0: u32 = 0x0000_0007; // ch0 clock select (bits 2:0)

// PWM Control Register
pub const REG_PCR: u32 = PWM_BA + 0x008;
    pub const CH3MOD: u32 = 0x0800_0000; // ch3 toggle/one-shot
    pub const CH3INV: u32 = 0x0400_0000; // ch3 inverter
    pub const CH3EN: u32 = 0x0100_0000;  // ch3 enable
    pub const CH2MOD: u32 = 0x0008_0000; // ch2 toggle/one-shot
    pub const CH2INV: u32 = 0x0004_0000; // ch2 inverter
    pub const CH2EN: u32 = 0x0001_0000;  // ch2 enable
    pub const CH1MOD: u32 = 0x0000_0800; // ch1 toggle/one-shot
    pub const CH1INV: u32 = 0x0000_0400; // ch1 inverter
    pub const CH1EN: u32 = 0x0000_0100;  // ch1 enable
    pub const DZEN1: u32 = 0x0000_0020;  // Dead-zone 1 enable
    pub const DZEN0: u32 = 0x0000_0010;  // Dead-zone 0 enable
    pub const CH0MOD: u32 = 0x0000_0008; // ch0 toggle/one-shot
    pub const CH0INV: u32 = 0x0000_0004; // ch0 inverter
    pub const CH0EN: u32 = 0x0000_0001;  // ch0 enable

// Per-channel register offsets (stride = 12 bytes: CNR, CMR, PDR)
pub const REG_CNR0: u32 = PWM_BA + 0x00C;
pub const REG_CNR1: u32 = PWM_BA + 0x018;
pub const REG_CNR2: u32 = PWM_BA + 0x024;
pub const REG_CNR3: u32 = PWM_BA + 0x030;

pub const REG_CMR0: u32 = PWM_BA + 0x010;
pub const REG_CMR1: u32 = PWM_BA + 0x01C;
pub const REG_CMR2: u32 = PWM_BA + 0x028;
pub const REG_CMR3: u32 = PWM_BA + 0x034;

pub const REG_PDR0: u32 = PWM_BA + 0x014;
pub const REG_PDR1: u32 = PWM_BA + 0x020;
pub const REG_PDR2: u32 = PWM_BA + 0x02C;
pub const REG_PDR3: u32 = PWM_BA + 0x038;

// PWM Interrupt Enable Register
pub const REG_PIER: u32 = PWM_BA + 0x040;
    pub const PIER3: u32 = 0x08; // ch3 interrupt enable
    pub const PIER2: u32 = 0x04; // ch2 interrupt enable
    pub const PIER1: u32 = 0x02; // ch1 interrupt enable
    pub const PIER0: u32 = 0x01; // ch0 interrupt enable

// PWM Interrupt Identification Register (write 1 to clear)
pub const REG_PIIR: u32 = PWM_BA + 0x044;
    pub const PIIR3: u32 = 0x08; // ch3 interrupt flag
    pub const PIIR2: u32 = 0x04; // ch2 interrupt flag
    pub const PIIR1: u32 = 0x02; // ch1 interrupt flag
    pub const PIIR0: u32 = 0x01; // ch0 interrupt flag

// Capture Control Register 0 (ch 0/1)
pub const REG_CCR0: u32 = PWM_BA + 0x050;
    pub const CFLRD1: u32 = 0x0080_0000; // CFLR1 dirty
    pub const CRLRD1: u32 = 0x0040_0000; // CRLR1 dirty
    pub const CIIR1: u32 = 0x0010_0000;  // Capture int indication 1
    pub const CAPCH1EN: u32 = 0x0008_0000; // Capture ch1 enable
    pub const FL_IE1: u32 = 0x0004_0000;  // ch1 falling int enable
    pub const RL_IE1: u32 = 0x0002_0000;  // ch1 rising int enable
    pub const INV1: u32 = 0x0001_0000;    // ch1 inverter
    pub const CFLRD0: u32 = 0x0000_0080; // CFLR0 dirty
    pub const CRLRD0: u32 = 0x0000_0040; // CRLR0 dirty
    pub const CIIR0: u32 = 0x0000_0010;  // Capture int indication 0
    pub const CAPCH0EN: u32 = 0x0000_0008; // Capture ch0 enable
    pub const FL_IE0: u32 = 0x0000_0004;  // ch0 falling int enable
    pub const RL_IE0: u32 = 0x0000_0002;  // ch0 rising int enable
    pub const INV0: u32 = 0x0000_0001;    // ch0 inverter

// Capture Control Register 1 (ch 2/3)
pub const REG_CCR1: u32 = PWM_BA + 0x054;
    pub const CFLRD3: u32 = 0x0080_0000;
    pub const CRLRD3: u32 = 0x0040_0000;
    pub const CIIR3: u32 = 0x0010_0000;
    pub const CAPCH3EN: u32 = 0x0008_0000;
    pub const FL_IE3: u32 = 0x0004_0000;
    pub const RL_IE3: u32 = 0x0002_0000;
    pub const INV3: u32 = 0x0001_0000;
    pub const CFLRD2: u32 = 0x0000_0080;
    pub const CRLRD2: u32 = 0x0000_0040;
    pub const CIIR2: u32 = 0x0000_0010;
    pub const CAPCH2EN: u32 = 0x0000_0008;
    pub const FL_IE2: u32 = 0x0000_0004;
    pub const RL_IE2: u32 = 0x0000_0002;
    pub const INV2: u32 = 0x0000_0001;

// Capture Rising Latch Registers
pub const REG_CRLR0: u32 = PWM_BA + 0x058;
pub const REG_CRLR1: u32 = PWM_BA + 0x060;
pub const REG_CRLR2: u32 = PWM_BA + 0x068;
pub const REG_CRLR3: u32 = PWM_BA + 0x070;

// Capture Falling Latch Registers
pub const REG_CFLR0: u32 = PWM_BA + 0x05C;
pub const REG_CFLR1: u32 = PWM_BA + 0x064;
pub const REG_CFLR2: u32 = PWM_BA + 0x06C;
pub const REG_CFLR3: u32 = PWM_BA + 0x074;

// Capture Input Enable Register
pub const REG_CAPENR: u32 = PWM_BA + 0x078;
    pub const CAPEN3: u32 = 0x08; // Capture input 3 enable
    pub const CAPEN2: u32 = 0x04; // Capture input 2 enable
    pub const CAPEN1: u32 = 0x02; // Capture input 1 enable
    pub const CAPEN0: u32 = 0x01; // Capture input 0 enable

// PWM Output Enable Register
pub const REG_POE: u32 = PWM_BA + 0x07C;
    pub const POE3: u32 = 0x08; // PWM output 3 enable
    pub const POE2: u32 = 0x04; // PWM output 2 enable
    pub const POE1: u32 = 0x02; // PWM output 1 enable
    pub const POE0: u32 = 0x01; // PWM output 0 enable

// Clock divider codes for REG_PWM_CSR
pub const PWM_CSR_DIV1: u32 = 4;  // /1
pub const PWM_CSR_DIV2: u32 = 0;  // /2
pub const PWM_CSR_DIV4: u32 = 1;  // /4
pub const PWM_CSR_DIV8: u32 = 2;  // /8
pub const PWM_CSR_DIV16: u32 = 3; // /16

// IRQ number
pub const IRQ_PWM: u32 = 25;
pub const IRQ_I2C: u32 = 30;

// ============================================================================
// I2C Registers (I2C_BA = 0xB800_4000)
// ============================================================================

// Control and Status Register
pub const REG_I2C_CSR: u32 = I2C_BA + 0x00;
    pub const I2C_RXACK: u32 = 0x0800;  // Received ACK from slave
    pub const I2C_BUSY: u32 = 0x0400;   // I2C bus busy
    pub const I2C_AL: u32 = 0x0200;     // Arbitration lost
    pub const I2C_TIP: u32 = 0x0100;    // Transfer in progress
    pub const TX_NUM: u32 = 0x0030;     // Transmit byte count (bits 5:4)
    pub const CSR_IF: u32 = 0x0004;     // Interrupt flag
    pub const CSR_IE: u32 = 0x0002;     // Interrupt enable
    pub const I2C_EN: u32 = 0x0001;     // I2C core enable

// Clock Prescale Register
pub const REG_I2C_DIVIDER: u32 = I2C_BA + 0x04;

// Command Register
pub const REG_I2C_CMDR: u32 = I2C_BA + 0x08;
    pub const I2C_START: u32 = 0x10;  // Generate start
    pub const I2C_STOP: u32 = 0x08;   // Generate stop
    pub const I2C_READ: u32 = 0x04;   // Read from slave
    pub const I2C_WRITE: u32 = 0x02;  // Write to slave
    pub const I2C_ACK: u32 = 0x01;    // Send ACK

// Software Mode Register
pub const REG_I2C_SWR: u32 = I2C_BA + 0x0C;
    pub const I2C_SER: u32 = 0x20;  // SDO status
    pub const I2C_SDR: u32 = 0x10;  // SDA status
    pub const I2C_SCR: u32 = 0x08;  // SCK status
    pub const I2C_SEW: u32 = 0x04;  // SDO output control
    pub const I2C_SDW: u32 = 0x02;  // SDA output control
    pub const I2C_SCW: u32 = 0x01;  // SCK output control

// Data registers
pub const REG_I2C_RxR: u32 = I2C_BA + 0x10; // Receive
pub const REG_I2C_TxR: u32 = I2C_BA + 0x14; // Transmit

// ============================================================================
// MMIO Helper Functions
// ============================================================================

/// Write 32-bit value to MMIO register
#[inline(always)]
pub unsafe fn reg_write32(addr: u32, value: u32) {
    core::ptr::write_volatile(addr as *mut u32, value);
}

/// Read 32-bit value from MMIO register
#[inline(always)]
pub unsafe fn reg_read32(addr: u32) -> u32 {
    core::ptr::read_volatile(addr as *const u32)
}

/// Write 16-bit value to MMIO register
#[inline(always)]
pub unsafe fn reg_write16(addr: u32, value: u16) {
    core::ptr::write_volatile(addr as *mut u16, value);
}

/// Read 16-bit value from MMIO register
#[inline(always)]
pub unsafe fn reg_read16(addr: u32) -> u16 {
    core::ptr::read_volatile(addr as *const u16)
}

/// Write 8-bit value to MMIO register
#[inline(always)]
pub unsafe fn reg_write8(addr: u32, value: u8) {
    core::ptr::write_volatile(addr as *mut u8, value);
}

/// Read 8-bit value from MMIO register
#[inline(always)]
pub unsafe fn reg_read8(addr: u32) -> u8 {
    core::ptr::read_volatile(addr as *const u8)
}
