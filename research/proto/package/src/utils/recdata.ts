import { KeyTouchMode, OrderType } from '../constants/param';
import { DefKeyValue, IDB, IDefKeyInfo, ILightMode, Keys } from '../types/interface';
import { compareVersions, preciseCalculate } from './index';

const {
  ORDER_TYPE_KEYBOARD_NAME,
  ORDER_TYPE_PRECISION_STROKE,
  ORDER_TYPE_PROTOCOL_VERSION,
  ORDER_TYPE_ROES,
  ORDER_TYPE_CONFIG,
  ORDER_TYPE_AXOSOME,
  ORDER_TYPE_CURRENT_AXOSOME,
  ORDER_TYPE_SET_WIN_MODEL,
  ORDER_TYPE_SET_MAC_MODEL,
  ORDER_TYPE_QUERY_MAC_MODEL,
  ORDER_TYPE_QUERY_WIN_MODEL,
  ORDER_TYPE_TOP_DEAD_SWITCH,
  QUERY_LIGHT_FIX_RGB,
  ORDER_TYPE_RGBNACK,
} = OrderType;

export const getCmdSyncRecdata = (data: Uint8Array) => {
  const value = {
    BoardID: 0,
    KeyboardLayout: 0,
    KeyType: 0,
    CustomerID: 0,
    ProductionId: 0,
    KeyboardRunMode: 0,
    KeyboardSN: '',
    firewareSpaceSize: 0,
    appVersion: '',
    appBuildDate: '',
    versionString: '',
  };
  // console.log('getCmdSyncRecdata', data);
  // 获取板子ID
  const buf = data.slice(1, 5);
  const view = new DataView(buf.buffer); // 创建一个 DataView 对象
  value.BoardID = view.getUint32(0, true); // 读取 4 字节的整数（小端序） true 表示小端序
  const [, , , KeyType, KeyboardLayout, , , KeyboardRunMode] = data;
  // 获取键盘布局
  value.KeyboardLayout = KeyboardLayout;

  // 获取键盘轴体
  value.KeyType = KeyType;

  // 获取客户ID(暂时留空)

  // 获取产品ID(暂时留空)

  // 获取键盘运行模式
  value.KeyboardRunMode = KeyboardRunMode;
  // 获取键盘SN
  const decoder = new TextDecoder('utf-8');
  value.KeyboardSN = decoder.decode(data.slice(9, 25));
  // 获取硬件版本
  const hardVersion = (data[6] << 8) | data[5];

  if (hardVersion < 1000) {
    // 十六进制0x110 1.1.0
    // 将 hardVersion 转换为版本号
    const majorVersion = Math.floor(hardVersion / 100); // 百位
    const minorVersion = Math.floor((hardVersion % 100) / 10); // 十位
    const patchVersion = hardVersion % 10; // 个位
    // 将版本号格式化为字符串，如 "X.Y.Z"
    value.versionString = `V${majorVersion}.${minorVersion}.${patchVersion}`;
  } else {
    value.firewareSpaceSize = hardVersion * 256;
  }
  // 获取固件版本
  const versionBuff = decoder.decode(data.slice(26, 30));
  if (versionBuff.startsWith('Boot')) {
    value.appVersion = decoder.decode(data.slice(26, 37));
  } else {
    value.appVersion = decoder.decode(data.slice(26, 36));
  }
  // 获取固件编译日期(数据不全，暂时不用)
  value.appBuildDate = decoder.decode(data.slice(43, 54));
  return value;
};
export const getCmdRecdata = (data: Uint8Array) => {
  const order = data[1];

  if (order === ORDER_TYPE_KEYBOARD_NAME) {
    const decoder = new TextDecoder('utf-8');
    const filteredData = data.slice(2, 34).filter((byte) => byte !== 0x00);
    // 解码过滤后的数据
    const KeyboardName = decoder.decode(new Uint8Array(filteredData));
    return KeyboardName;
  }
  if (order === ORDER_TYPE_PROTOCOL_VERSION) {
    // 第一个字节的低4位是版本号的第一位
    const version1 = data[3] & 0x0f;
    // 第二个字节的高4位是版本号的第二位
    const version2 = (data[2] >> 4) & 0x0f;
    // 第二个字节的低4位是版本号的第三位
    const version3 = data[2] & 0x0f;

    return `${version1}.${version2}.${version3}`;
  }
  if (order === ORDER_TYPE_PRECISION_STROKE) {
    const value = { precision: 0, decimalPlace: 0, minTouchTravel: 0, maxTouchTravel: 0, VID: 0, PID: 0 };
    const precision = data[2] / 1000;
    value.precision = precision;

    const min = (data[4] << 8) | data[3];
    value.minTouchTravel = min / 1000;

    const max = (data[6] << 8) | data[5];
    value.maxTouchTravel = max / 1000;

    // 小数位

    const str = precision.toString();
    const decimalPart = str.split('.')[1];
    value.decimalPlace = decimalPart.length;

    // TODO:
    // if ((VID.value === 0x373b && PID.value === 0x10b4) || (VID.value === 0x373b && PID.value === 0x205a)) {
    //   precision.value = 0.01;
    //   decimalPlace.value = 2;
    // }
    return value;
  }
  if (order === ORDER_TYPE_ROES) {
    const rateOfReturn = data[2];
    return rateOfReturn;
  }
  if (order === ORDER_TYPE_CONFIG) {
    const configID = data[2];
    return { configID, hasFourConfig: true };
  }
  if (order === ORDER_TYPE_AXOSOME) {
    const axisList = [];

    for (let i = 0; i < 8; i++) {
      const id = (data[(i + 1) * 2] << 8) | data[(i + 1) * 2 + 1];
      if (id === 0xffff) {
        break;
      } else {
        axisList.push(id);
      }
    }

    return { hasAxisSetting: true, axisList };
  }
  if (order === ORDER_TYPE_CURRENT_AXOSOME) {
    return (data[3] << 8) | data[2];
  }
  if (order === ORDER_TYPE_SET_WIN_MODEL) {
    return data[2] === 1 ? 0 : null;
  }
  if (order === ORDER_TYPE_SET_MAC_MODEL) {
    return data[2] === 1 ? 1 : null;
  }
  if (order === ORDER_TYPE_QUERY_WIN_MODEL) {
    const value = { currentSystem: '', hasWinMode: false };
    const key = data[2];
    if (key === 1) {
      value.currentSystem = 'win';
      value.hasWinMode = true;
    } else if (key === 0) {
      value.currentSystem = 'mac';
      value.hasWinMode = true;
    } else if (key === 0xff) {
      value.hasWinMode = false;
    }
    return value;
  }
  if (order === ORDER_TYPE_QUERY_MAC_MODEL) {
    const value = { currentSystem: 'mac', hasMacMode: false };
    const key = data[2];
    if (key === 1) {
      value.currentSystem = 'mac';
      value.hasMacMode = true;
    } else if (key === 0) {
      value.currentSystem = 'win';
      value.hasMacMode = true;
    } else if (key === 0xff) {
      value.hasMacMode = false;
    }
    return value;
  }
  if (order === ORDER_TYPE_TOP_DEAD_SWITCH) {
    const value = data[2] !== 0;
    return value;
  }
  if (order === QUERY_LIGHT_FIX_RGB) {
    const rgb = {
      r: data[2],
      g: data[3],
      b: data[4],
    };

    return rgb;
  }
  // 自定义是否有回包  1 打开自定义灯光回包，0 关闭自定义灯光回包
  if (order === ORDER_TYPE_RGBNACK) {
    const rgbNack = data[2];
    return rgbNack !== 0;
  }
  return null;
};

export const getPRGBRecdata = (data: Uint8Array) => {
  const lightMod: ILightMode = {
    open: false, // 灯光背景
    direction: false, // 方向 true 正向 false 反向
    superResponse: false, // 超强响应
    speed: 0, // 灯光速度
    colors: [], // 颜色组
    mode: 0, // 0 关闭, 1-20表示效果，21 自定义
    luminance: 0, // 亮度
    sleepDelay: 0, // 灯光休眠时间
    staticColor: 0, // 静态灯光颜色模式
    type: 'static',
    dynamicColorId: 0,
  };
  let bit = 5;
  for (let i = 0; i < 7; i++) {
    const B = data[bit++];
    const G = data[bit++];
    const R = data[bit++];
    bit++;
    // console.log(`R:${R}  G:${G}  B:${B}`);
    const color = `#${R.toString(16).padStart(2, '0')}${G.toString(16).padStart(2, '0')}${B.toString(16).padStart(2, '0')}`;
    lightMod.colors.push(color);
  }
  // 获取灯光
  const bitmap = data[37];
  // 提取灯光开关（第0位）
  lightMod.open = (bitmap & (0x01 << 0)) !== 0;
  // 提取动态灯效方向（第1位）
  lightMod.direction = (bitmap & (0x01 << 1)) !== 0;
  // 提取超级响应开关（第4位）
  lightMod.superResponse = (bitmap & (0x01 << 4)) !== 0;
  // 获取灯光亮度
  const luminance = data[38];
  lightMod.luminance = luminance;
  // 获取灯光速度
  const speed = data[40];
  lightMod.speed = speed;
  // 获取灯光模式
  const mode = data[39];
  lightMod.mode = mode;

  if (mode === 0) {
    lightMod.type = 'static';
  } else if (mode > 0 && mode <= 20) {
    lightMod.type = 'dynamic';
  } else {
    lightMod.type = 'custom';
  }
  // 获取灯光睡眠时间
  const sleepDelay = data[41];
  lightMod.sleepDelay = sleepDelay;

  // 获取静态灯效模式
  const staticColor = data[42];
  lightMod.staticColor = staticColor;

  // 获取动态灯效模式
  const dynamicColorId = data[43];
  lightMod.dynamicColorId = dynamicColorId;
  return lightMod;
};

// 特殊的RGBRecdata

export const getSpecialPRGBRecdata = (data: Uint8Array) => {
  const lightMod: ILightMode = {
    open: false, // 灯光背景
    direction: false, // 方向 true 正向 false 反向
    superResponse: false, // 超强响应
    speed: 0, // 灯光速度
    colors: [], // 颜色组
    mode: 0, // 0 关闭, 1-20表示效果，21 自定义
    luminance: 0, // 亮度
    sleepDelay: 0, // 灯光休眠时间
    staticColor: 0, // 静态灯光颜色模式
    type: 'static',
    dynamicColorId: 0,
  };
  let bit = 5;
  for (let i = 0; i < 7; i++) {
    const B = data[bit++];
    const G = data[bit++];
    const R = data[bit++];
    bit++;
    // console.log(`R:${R}  G:${G}  B:${B}`);
    const color = `#${R.toString(16).padStart(2, '0')}${G.toString(16).padStart(2, '0')}${B.toString(16).padStart(2, '0')}`;
    lightMod.colors.push(color);
  }
  // 获取灯光
  const bitmap = data[37];
  // 提取灯光开关（第0位）
  lightMod.open = (bitmap & (0x01 << 0)) !== 0;
  // 提取动态灯效方向（第1位）
  lightMod.direction = (bitmap & (0x01 << 1)) !== 0;
  // 提取超级响应开关（第4位）
  lightMod.superResponse = (bitmap & (0x01 << 4)) !== 0;
  // 获取灯光亮度
  const luminance = data[38];
  lightMod.luminance = luminance;
  // 获取灯光速度
  const speed = data[40];
  lightMod.speed = speed;
  // 获取灯光模式
  const mode = data[39];
  lightMod.mode = mode;

  if (mode === 0) {
    lightMod.type = 'static';
  } else if (mode > 0 && mode <= 20) {
    lightMod.type = 'dynamic';
  } else {
    lightMod.type = 'custom';
  }
  // 获取灯光睡眠时间
  const sleepDelay = data[41];
  lightMod.sleepDelay = sleepDelay;

  // 获取静态灯效模式
  const staticColor = data[42];
  lightMod.staticColor = staticColor;

  // 获取动态灯效模式
  const dynamicColorId = data[43];
  lightMod.dynamicColorId = dynamicColorId;
  return lightMod;
};

export const getSingleRGBRecdata = (data: Uint8Array) => {
  const key = data[1];
  const R = data[2];
  const G = data[3];
  const B = data[4];
  return { key, R, G, B };
};
export const getDefKeyRecdata = (data: Uint8Array): DefKeyValue => {
  // 定义 KeyInfo 类型

  // 该数据解包返回的是一个二维数组，每次返回两行，针对键盘布局不一样，返回行中每一个数据对应的就是列值
  const firstRow: number = data[1];
  const secondRow: number = data[23];
  // data 从下标2开始取21个数据
  const firstRowData: Uint8Array = data.slice(2, 23);
  const secondRowData: Uint8Array = data.slice(24, 45);
  const value: DefKeyValue = [[], []];

  firstRowData.forEach((keyValue, index) => {
    const keyInfo: IDefKeyInfo = { keyValue, location: { row: firstRow, col: index } };
    if (keyValue !== 0) value[0].push(keyInfo);
  });

  secondRowData.forEach((keyValue, index) => {
    const keyInfo: IDefKeyInfo = { keyValue, location: { row: secondRow, col: index } };
    if (keyValue !== 0) value[1].push(keyInfo);
  });
  return value;
};
export const getFnLayoutKeyRecdata = (data: Uint8Array): Keys => {
  const keys = [];
  for (let i = 0; i < data.length; i += 4) {
    const key = data[i + 1];
    const layout = data[i + 2];
    const value = (data[i + 4] << 8) | data[i + 3];
    // 三个值不等于255
    if (layout !== 255 && value !== 255 && key !== 0) keys.push({ key, layout, value });
  }
  return keys;
};
export const getGlobalTouchTravelRecdata = (data: Uint8Array) => {
  const db: IDB = { globalTouchTravel: 0, pressDead: 0, releaseDead: 0 };
  db.globalTouchTravel = ((data[4] << 8) | data[3]) / 1000.0;
  db.pressDead = ((data[6] << 8) | data[5]) / 1000.0;
  db.releaseDead = ((data[8] << 8) | data[7]) / 1000.0;
  return db;
};
export const getLayoutModelRecdata = (data: Uint8Array) => {
  const value = (data[4] << 8) | data[3];
  const uint8Value = value & 0xff;
  const touchValue = (uint8Value >> 4) & 0x0f;
  const advancedKeyValue = uint8Value & 0x0f;
  let touchMode = '';
  switch (touchValue) {
    case KeyTouchMode.global:
      touchMode = 'global';
      break;
    case KeyTouchMode.single:
      touchMode = 'single';
      break;
    case KeyTouchMode.rt:
      touchMode = 'rt';
      break;
    default:
      touchMode = 'global';
      break;
  }
  // TODO:高级键模式有宏、socd、rs、tgl、end、dks、mpt、mt
  const advancedKeyMode = advancedKeyValue;
  return { touchMode, advancedKeyMode };
};
export const getSingleTravelRecdata = (data: Uint8Array, decimal: number) => {
  const value = (data[4] << 8) | data[3];
  const touchTravel = value / 1000.0;
  return touchTravel.toFixed(decimal);
};
export const getDksTravelRecdata = (data: Uint8Array) => {
  const value = (data[4] << 8) | data[3];
  return preciseCalculate('divide', value, 1000);
};
export const getRtTravelRecdata = (data: Uint8Array) => {
  const value = (data[4] << 8) | data[3];
  return preciseCalculate('divide', value, 1000);
};
export const getDpDrRecdata = (data: Uint8Array) => {
  const value = (data[4] << 8) | data[3];
  return value / 1000.0;
};
export const getAxisRecdata = (data: Uint8Array) => {
  const axis = (data[4] << 8) | data[3];
  return { axis };
};
export const getMTRecdata = (data: Uint8Array) => {
  return data;
};
export const getTrpsRecdata = (data: Uint8Array) => {
  const value = (data[4] << 8) | data[3];
  return { trps: value };
};
export const getMtorTglRecdata = (data: Uint8Array) => {
  const value = (data[4] << 8) | data[3];
  return value * 10;
};
// 高级键
export const getDksRecdata = (data: Uint8Array) => {
  const value = (data[4] << 8) | data[3];
  return { dks: value };
};
export const getMptRecdata = (data: Uint8Array) => {
  const dks1 = (data[3] << 8) | data[2];
  const dks2 = (data[5] << 8) | data[4];
  const dks3 = (data[7] << 8) | data[6];
  const db1 = (data[9] << 8) | data[8];
  const db2 = (data[11] << 8) | data[10];
  const db3 = (data[13] << 8) | data[12];
  return {
    dks: [dks1, dks2, dks3],
    dbs: [
      preciseCalculate('divide', db1, 1000),
      preciseCalculate('divide', db2, 1000),
      preciseCalculate('divide', db3, 1000),
    ],
  };
};
export const getMtRecdata = (data: Uint8Array) => {
  return data;
};
export const getTglRecdata = (data: Uint8Array) => {
  const dks = (data[3] << 8) | data[2];
  const delay = data[4];
  return { dks, delay: delay * 10 };
};

export const getEndRecdata = (data: Uint8Array) => {
  const dks = (data[3] << 8) | data[2];
  const delay = (data[5] << 8) | data[4];
  return { dks, delay };
};

// v3版本
export const getSocdRecdata = (data: Uint8Array, v: string = '1.0.5') => {
  const greaterOrEqual107Tag = compareVersions(v, '1.0.7');
  const isGreaterOrEqual107Tag = ['greater', 'equal'].includes(greaterOrEqual107Tag);
  const dksv1 = (data[4] << 8) | data[3];
  const dksv2 = (data[6] << 8) | data[5];
  if (isGreaterOrEqual107Tag) {
    const delay = (data[10] << 8) | data[9];
    return { pos1: data[1], pos2: data[2], key1: dksv1, key2: dksv2, type: data[7], mode: data[8], delay };
  }
  return { pos: data[1], key: dksv1, type: data[7], mode: data[8] };
};

export const getRsRecdata = (data: Uint8Array) => {
  console.log('data', data);
  const dks1 = data[1];
  const dks2 = data[2];
  return { dks1, dks2 };
};
export const getMacroRecdata = (data: Uint8Array) => {
  const key = data[1];
  const id = (data[3] << 8) | data[2];
  const len = data[4];
  const mode = data[5];
  const num = (data[7] << 8) | data[6];
  const delay = (data[10] << 16) | (data[9] << 8) | data[8];
  return { key, id, len, mode, num, delay };
};
export const getMacroDataRecdata = (data: Uint8Array) => {
  return data;
};
// 图片的解包
export const getPicRecdata = (data: Uint8Array) => {
  if (data[0] === 0) {
    // 获取当前升级的地址
    const addrBuff = (data[4] << 24) | (data[3] << 16) | (data[2] << 8) | data[1];
    const sizeBuff = (data[6] << 8) | data[5];
    return { addrBuff, sizeBuff };
  }
  return null;
};

export const getRm6X21Recdata = (data: Uint8Array) => {
  // 开始取值的位置
  const dataSlice = data.slice(2);
  const normalArray = Array.from(dataSlice);
  if (data[1] === 0x03) {
    const value = [];
    for (let i = 0; i < normalArray.length; i += 21) {
      // 连续两个255的值终止循环
      if (normalArray[i] === 255 && normalArray[i + 1] === 255) break;
      const chunk = normalArray.slice(i, i + 21);
      if (chunk.length === 21) value.push(chunk);
    }
    return value;
    // 计算一个最大值。排除掉可能是第一行存在没有数据的情况
  }
  if (data[1] === 0x02 || data[1] === 0x06) {
    const value = [];
    // 168次
    for (let i = 0; i < 3; i++) {
      value.push([]);
      for (let j = 0; j < 21; j++) {
        const index = i * 2 * 21 + j * 2;
        const buff = (normalArray[index + 1] << 8) | normalArray[index];
        const buffValue = preciseCalculate('divide', buff, 1000);
        value[i].push(buffValue);
      }
    }
    return value;
  }
};

export const getSignRecdata = (data: Uint8Array) => {
  if (data[0] === 0) {
    const signature = [];
    for (let i = 0; i < 16; i++) {
      signature[i] = data[i + 3];
    }
    return { signSuccess: true, signature };
  }
  return { signSuccess: false, signature: [] };
};

export const getWriteRecdata = (data: Uint8Array) => {
  if (data[0] === 0) {
    // 获取当前升级的地址
    const addrBuff = (data[4] << 24) | (data[3] << 16) | (data[2] << 8) | data[1];
    const sizeBuff = (data[6] << 8) | data[5];

    return { currentUpdateAddress: addrBuff + sizeBuff };
  }
  return { currentUpdateAddress: 0 };
};

export const getCrcRecdata = (data: Uint8Array) => {
  return (data[10] << 8) | data[9];
};
