import { OrderType } from '../constants/param';

export interface IDefKeyInfo {
  keyValue: number; // 键值
  location: {
    row: number; // 行
    col: number; // 列
  };
}

// 定义 Value 类型
export type DefKeyValue = [IDefKeyInfo[], IDefKeyInfo[]]; // Value 是一个包含两个 KeyInfo 数组的元组

export interface IEnd {
  keys: number;
  dks: number;
}

export interface IKRGBDesc {
  key: number; // assumed length为7的数组
  r?: number;
  g?: number;
  b?: number;
}

export type KRGBDescs = IKRGBDesc[];

export type cmdType = keyof typeof OrderType;

export interface ICmd {
  type: cmdType; // 命令顺序
  hArgs?: number[];
  is8bit?: boolean;
}

export interface IKey {
  key: number; // 2b
  layout: number;
  value?: number;
}

export type Keys = IKey[];

export type LightModeType = 'static' | 'custom' | 'dynamic';

export type LightLogoModeType = 'static' | 'dynamic';

export interface ILightMode {
  open: boolean; // 灯光背景
  direction: boolean; // 方向 true 正向 false 反向
  superResponse: boolean; // 超强响应
  speed: number; // 灯光速度
  colors: string[]; // 颜色组
  mode: number; // 0 关闭, 1-20表示效果，21 自定义
  luminance: number; // 亮度
  sleepDelay: number; // 灯光休眠时间
  staticColor: number; // 静态灯光颜色模式
  type?: LightModeType;
  dynamicColorId?: number;
}

export interface ILoGoLightMode {
  open: boolean; // 灯光背景
  direction: boolean; // 方向 true 正向 false 反向
  superResponse: boolean; // 超强响应
  speed: number; // 灯光速度
  colors: string[]; // 颜色组
  mode: number; // 0 关闭, 1-20表示效果，21 自定义
  luminance: number; // 亮度
  sleepDelay: number; // 灯光休眠时间
  staticColor: number; // 静态灯光颜色模式
  type?: LightLogoModeType;
}

export interface IDB {
  globalTouchTravel: number; // 0x01 ~ 0xFA0
  pressDead: number;
  releaseDead: number;
}

export interface IRGBDesc {
  lightColors: number[];
  lightSwitch: number;
  reverseEffect: number;
  superResponse: number;
  lightLuminance: number;
  lightMode: number;
  lightSpeed: number;
  lightSleepDelay: number;
  color: number;
}

export interface IKeyData {
  keys: number[];
  layouts: number[];
  values: number[];
}

export interface IDKSMode {
  key: number;
  dks: number[];
  trps: number[];
  dbs: number[];
}

export interface IMPTMode {
  key: number;
  dks?: number[];
  dbs?: number[];
}

export interface IMTMode {
  key: number;
  dks: number[];
  delay: number;
}

export interface ITGLMode {
  key: number;
  dks?: number;
  delay?: number;
}

export interface IEndMode {
  key: number;
  dks?: number;
  delay?: number;
}

export interface ISOCDMode {
  key?: number;
  dks1?: number;
  mode1?: number;
  mode2?: number;
}
export interface ISOCDModeV2 {
  pos1: number;
  pos2: number;
  key1: number;
  key2: number;
  type: number;
  mode: number;
}
export interface ISOCDModeV3 {
  pos1: number;
  pos2: number;
  key1: number;
  key2: number;
  type: number;
  mode: number;
  delay: number;
}

export interface IRSMode {
  key: number;
  dks: number;
}

export interface IMacroMode {
  key: number;
  index: number;
  len: number;
  mode: number;
  num: number;
  delay: number;
}

export interface IRM6X21Mode {
  matrix6x21: number;
  datatype: number;
}

export interface IModeParam {
  key1: any; // 可以替换为更具体的类型
  dks1: any; // 可以替换为更具体的类型
  dks2: any; // 可以替换为更具体的类型
  delay: number; // 假设 delay 是一个数字
}

export interface IWriteParam {
  addr: number;
  size: number;
  codes: number[];
}

export type VersionString = `${number}.${number}.${number}`;
