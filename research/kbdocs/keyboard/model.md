# 模型

## 设备

```ts
interface HIDCollectionInfo {
  usagePage: number;
  usage: number;
  inputReports: ReadonlyArray<HIDReportInfo>;
  outputReports: ReadonlyArray<HIDReportInfo>;
  featureReports: ReadonlyArray<HIDReportInfo>;
  children: ReadonlyArray<HIDCollectionInfo>;
}

interface HIDDevice extends EventTarget {
  id: string;
  collections: ReadonlyArray<HIDCollectionInfo>;
  readonly opened: boolean;
  readonly vendorId: number;
  readonly productId: number;
  readonly productName: string;
  oninputreport: (event: Event) => void | null;
  open(): Promise<void>;
  close(): Promise<void>;
  forget(): Promise<void>;
  sendReport(reportId: number, data: BufferSource): Promise<void>;
  sendFeatureReport(reportId: number, data: BufferSource): Promise<void>;
  receiveFeatureReport(reportId: number): Promise<DataView>;
}

type Device = {
   data: HIDDevice, // 设备信息
   id: string, // 设备id
   usage: number, // usage
   usagePage: number, // usagePage
   vendorId: number, // vendorId
   productId: number, // productId
   productName: string, // productName
}

interface BaseInfo {
  BoardID: number;
  KeyboardLayout: number;
  KeyType: number;
  CustomerID: number;
  ProductionId: number;
  KeyboardRunMode: number;
  KeyboardSN: string;
  firewareSpaceSize: number;
  appVersion: string;
  appBuildDate: string;
  versionString: string;
}

enum RateOfReturn {
  '8KHz' = 0,
  '4KHz' = 1,
  '2KHz' = 2,
  '1KHz' = 3,
  '500Hz' = 4,
  '250Hz' = 5,
  '125Hz' = 6,
}

interface Calibration {
  calibrations: number[][], // 包含所有按键的最大行程值
  travels: number[][], // 包含所有按键按下的行程值
}

```

## 布局模型

```ts
[
  IDefKeyInfo[],
  IDefKeyInfo[],
  IDefKeyInfo[],
  IDefKeyInfo[],
  IDefKeyInfo[],
  IDefKeyInfo[],

]
```

## 单键的布局模型

```ts
type IDefKeyInfo = {
  keyValue: number; // 键值
  location: {
    row: number; // 行
    col: number; // 列
  };
}
```

## 灯光

```ts

// 灯光模型
type LightMode = {
  open: boolean; // 是否开启灯光
  direction: boolean; // 方向 true 正向 false 反向
  superResponse: boolean; // 超强响应
  speed: number; // 灯光速度
  colors: string[]; // 颜色组
  mode: number; // 0 关闭, 1-20表示效果，21 自定义
  luminance: number; // 亮度
  sleepDelay: number; // 灯光休眠时间
  staticColor: number; // 静态灯光颜色模式
  type: LightModeType;
  // LightModeType = 'static' | 'custom' | 'dynamic';   
  // type的类型：custom 自定义模式，static 静态模式，dynamic 动态模式。
  // 当设置Logo灯的时候type只有static静态模式和dynamic动态模式两种模式。
};

// 灯光Type
type LightModeType = 'custom' | 'static' | 'dynamic';

```

## 高级键

```ts
type TrpsLayoutType = 'Layout_TRPS1' | 'Layout_TRPS2' | 'Layout_TRPS3' | 'Layout_TRPS4';
interface IDKSMode {
  key: number;
  dks: number[];
  trps: number[];
  dbs: number[];
}
interface ISOCDMode {
  key?: number;
  dks1?: number;
  mode1?: number;
  mode2?: number;
}
interface ISOCDModeV2 {
  pos1: number;
  pos2: number;
  key1: number;
  key2: number;
  type: number;
  mode: number;
}
interface ISOCDModeV3 {
  pos1: number; // 原键值
  pos2: number; // 原键值
  key1: number; // 发送键值
  key2: number; // 发送键值
  type: number; // 发送键值类型 0=按pos发送键值，1=按Key发送键值，默认:0
  mode: number; // 发送键值模式 mode表示键值的发送模式，共有四种模式：0=后覆盖，1=a优先，2=b优先，3=中性（两个按键都按下都不生效）,默认:0
  delay: number; // 发送键值延迟
}
interface IEndMode {
  key: number;
  dks?: number;
  delay?: number;
}
```

## 宏

```ts

interface IMacroMode {
  key: number;
  index: number;
  len: number;
  mode: number;
  num: number;
  delay: number;
}

type MacroType = { keyCode: number; timeDifference: number; status: number };


```
