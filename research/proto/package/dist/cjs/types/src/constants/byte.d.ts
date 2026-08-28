export declare const Head: number;
export declare const MaxPack: number;
export declare enum Protocol {
    KB2_CMD = 0,
    KB2_CMD_SYNC = 1,
    KB2_CMD_KEY = 35,
    KB2_CMD_DB = 41,
    KB2_CMD_DEFKEY = 43,
    KB2_CMD_RM6X21 = 18,
    KB2_CMD_MT = 36,
    KB2_CMD_TGL = 37,
    KB2_CMD_DKS = 38,
    KB2_CMD_MPT = 39,
    KB2_CMD_END = 40,
    KB2_CMD_MACRO = 32,
    KB2_CMD_MACRO_MODE = 33,
    KB2_CMD_SOCD = 44,
    KB2_CMD_RS = 45,
    KB2_CMD_PRGB = 24,
    KB2_CMD_LOGORGB = 25,
    KB2_CMD_KRGB = 42,
    KB2_BL_SIGN = 8,
    KB2_BL_ERASE = 9,
    KB2_BL_REBOOT = 10,
    KB2_BL_TOAPP = 11,
    KB2_BL_WRITE = 12,
    KB2_BL_READ = 13,
    KB2_BL_RCRC = 14,
    KB2_CMD_FAIL = 255,
    KB2_CMD_PIC = 48,
    KB2_PIC_WRITE = 49
}
export declare enum KeyLayout {
    Layout_Fn0 = 0,
    Layout_Fn1 = 1,
    Layout_Fn2 = 2,
    Layout_Fn3 = 3,
    Layout_DB0 = 4,
    Layout_DB1 = 5,
    Layout_DB2 = 6,
    Layout_DB3 = 7,
    Layout_Mode = 8,
    Layout_DKS1 = 9,
    Layout_DKS2 = 10,
    Layout_DKS3 = 11,
    Layout_DKS4 = 12,
    Layout_TRPS1 = 13,
    Layout_TRPS2 = 14,
    Layout_TRPS3 = 15,
    Layout_TRPS4 = 16,
    Layout_MacroAddr = 17,
    Layout_MacroSize = 18,
    Layout_MTDelay = 19,
    Layout_RTP = 20,
    Layout_RTR = 21
}
export declare enum KeyTouchMode {
    GlobalMode = 0,
    SingleMode = 1,
    QuickMode = 2
}
export declare enum BLControls {
    BL_NONE = 0,
    BL_SIGN = 2,
    BL_ERASE = 3,
    BL_REBOOT = 4,
    BL_TOBOOT = 5,
    BL_WRITE = 6
}
