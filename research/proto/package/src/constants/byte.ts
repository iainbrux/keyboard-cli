export const Head: number = 0x5c;
export const MaxPack: number = 0x0e;

export enum Protocol {
  KB2_CMD = 0x00,
  KB2_CMD_SYNC = 0x01, // 同步
  KB2_CMD_KEY = 0x23, // 层数
  KB2_CMD_DB = 0x29, // 全局行程
  KB2_CMD_DEFKEY = 0x2b, // 默认键值
  KB2_CMD_RM6X21 = 0x12, // 矩阵
  KB2_CMD_MT = 0x24, // MT
  KB2_CMD_TGL = 0x25, // TGL
  KB2_CMD_DKS = 0x26, // DKS
  KB2_CMD_MPT = 0x27, // MPT
  KB2_CMD_END = 0x28, // END
  KB2_CMD_MACRO = 0x20, // MACRO
  KB2_CMD_MACRO_MODE = 0x21, // MACRO模式
  KB2_CMD_SOCD = 0x2c, // SOCD
  KB2_CMD_RS = 0x2d, // RS
  KB2_CMD_PRGB = 0x18, // PRGB
  KB2_CMD_LOGORGB = 0x19, // rgb灯
  KB2_CMD_KRGB = 0x2a, // KRGB参数

  // 固件升级
  KB2_BL_SIGN = 0x08, // 签名
  KB2_BL_ERASE = 0x09, // 擦除
  KB2_BL_REBOOT = 0x0a, // 重启
  KB2_BL_TOAPP = 0x0b, // 跳转到app
  KB2_BL_WRITE = 0x0c, // 写指令
  KB2_BL_READ = 0x0d, // 读指令
  KB2_BL_RCRC = 0x0e, // 获取校验
  KB2_CMD_FAIL = 0xff, // 错误

  KB2_CMD_PIC = 0x30, // 图片开始
  KB2_PIC_WRITE = 0x31, // 图片写入
} // 使用 as const 保持对象的字面量类型

export enum KeyLayout {
  Layout_Fn0 = 0x00,
  Layout_Fn1 = 0x01,
  Layout_Fn2 = 0x02,
  Layout_Fn3 = 0x03,
  Layout_DB0 = 0x04,
  Layout_DB1 = 0x05,
  Layout_DB2 = 0x06,
  Layout_DB3 = 0x07,
  Layout_Mode = 0x08,
  Layout_DKS1 = 0x09,
  Layout_DKS2 = 0x0a,
  Layout_DKS3 = 0x0b,
  Layout_DKS4 = 0x0c,
  Layout_TRPS1 = 0x0d,
  Layout_TRPS2 = 0x0e,
  Layout_TRPS3 = 0x0f,
  Layout_TRPS4 = 0x10,
  Layout_MacroAddr = 0x11,
  Layout_MacroSize = 0x12,
  Layout_MTDelay = 0x13,
  Layout_RTP = 0x14,
  Layout_RTR = 0x15,
}

export enum KeyTouchMode {
  GlobalMode = 0x00,
  SingleMode = 0x01,
  QuickMode = 0x02,
}

export enum BLControls {
  BL_NONE = 0x00,
  BL_SIGN = 0x02,
  BL_ERASE = 0x03,
  BL_REBOOT = 0x04,
  BL_TOBOOT = 0x05,
  BL_WRITE = 0x06,
}
