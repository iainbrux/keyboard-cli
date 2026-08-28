import { OrderType } from '../constants/param';
import {
  ICmd,
  IDB,
  IDKSMode,
  IEndMode,
  IKey,
  IKRGBDesc,
  ILightMode,
  IMPTMode,
  IMTMode,
  IRSMode,
  ISOCDMode,
  ISOCDModeV2,
  ISOCDModeV3,
  ITGLMode,
  IWriteParam,
  Keys,
  KRGBDescs,
  VersionString,
} from '../types/interface';
import {
  compareVersions,
  computeHighLowByte,
  getLightBitmap,
  getSomeBits,
  highByte,
  highByte16,
  highByte24,
  lowByte,
  preciseCalculate,
} from './index';

// CMD
export const CMDPack = (param: ICmd): number[] => {
  console.log('paramparamparamparam', param);
  const { type, hArgs, is8bit } = param;
  const order = OrderType[type];
  const placeholder = getSomeBits(2, 0xff);
  let value: number[] = hArgs || [];

  if (is8bit) {
    const bit8: number[] = [];
    value.forEach((byte) => {
      bit8.push(...computeHighLowByte(byte));
    });
    value = bit8;
  }
  return [order, ...value, ...placeholder];
};

// SYN
export const SYNCPack = (): number[] => {
  const random = [0x01, 0x02, 0x03, 0x04]; // 4b
  const placeholder = getSomeBits(2, 0xff);
  return [...random, ...placeholder];
};

// Key
export const KeyDataPack = (param: Keys): number[] => {
  const pack: number[] = [];
  param.forEach((item: IKey) => {
    const { key, layout, value } = item;
    pack.push(key);
    pack.push(layout);
    pack.push(value ? lowByte(value) : 0x00); // 低字节
    pack.push(value ? highByte(value) : 0x00); // 高字节
  });
  return pack;
};
// layout
export const KeyLayoutDataPack = (param: IKey): number[] => {
  const pack: number[] = [];
  const { key, layout, value } = param;
  pack.push(key);
  pack.push(layout);
  pack.push(value ? lowByte(value) : 0x00); // 低字节
  pack.push(value ? highByte(value) : 0x00); // 高字节
  return pack;
};

// DB
export const DBDataPack = (dbDesc: IDB = { globalTouchTravel: 0, pressDead: 0, releaseDead: 0 }): number[] => {
  const { globalTouchTravel, pressDead, releaseDead } = dbDesc;
  const ticks = getSomeBits(2);
  const global = computeHighLowByte(globalTouchTravel);
  const press = computeHighLowByte(pressDead);
  const release = computeHighLowByte(releaseDead);
  const dbn = getSomeBits(6);
  return [...ticks, ...global, ...press, ...release, ...dbn];
};

// RGB
export const PRGBDatapack = (lightDesc: ILightMode, v: VersionString = '1.0.7'): any[] => {
  const { colors, open, direction, superResponse, luminance, mode, speed, sleepDelay, staticColor, dynamicColorId } =
    lightDesc;
  const bit = getSomeBits(4);
  const data: number[] = [];
  const ff = getSomeBits(1, 0xff);

  // 这个地方最多只能支持七个颜色 因为第八个是动态颜色ID
  // if (colors.length > 7) {
  //   throw new Error('最多支持七个颜色');
  // }
  // 让colors只能是七个颜色
  const colorsCount = Math.min(colors.length, 7);

  console.log('colorsCountcolorsCountcolorsCount', colors, colorsCount);

  for (let i = 0; i < colorsCount; i++) {
    const str = colors[i];
    data.push(parseInt(str.substring(5, 7), 16)); // 提取蓝色部分并转换为十进制b
    data.push(parseInt(str.substring(3, 5), 16)); // 提取绿色部分并转换为十进制g
    data.push(parseInt(str.substring(1, 3), 16)); // 提取红色部分并转换为十进制r
    data.push(...ff);
  }

  const c8ColorMode = getSomeBits(4);
  const lightBitmap = getLightBitmap(open, direction, superResponse);

  const basePack = [...bit, ...data, ...c8ColorMode, lightBitmap, luminance, mode, speed, sleepDelay, staticColor];
  // 大于等于1.0.9
  const greaterOrEqual109Tag = compareVersions(v, '1.0.9');
  const isGreaterOrEqual109Tag = ['greater', 'equal'].includes(greaterOrEqual109Tag);
  return isGreaterOrEqual109Tag ? [...basePack, dynamicColorId] : basePack;
};

// special RGB
export const SRGBDatapack = (lightDesc: ILightMode): any[] => {
  const { colors, open, direction, superResponse, luminance, mode, speed, sleepDelay, staticColor } = lightDesc;
  const bit = getSomeBits(4);
  const data: number[] = [];
  const ff = getSomeBits(1, 0xff);

  const colorsCount = Math.min(colors.length, 7);
  for (let i = 0; i < colorsCount; i++) {
    const str = colors[i];
    data.push(parseInt(str.substring(5, 7), 16)); // 提取蓝色部分并转换为十进制b
    data.push(parseInt(str.substring(3, 5), 16)); // 提取绿色部分并转换为十进制g
    data.push(parseInt(str.substring(1, 3), 16)); // 提取红色部分并转换为十进制r
    data.push(...ff);
  }

  const lightBitmap = getLightBitmap(open, direction, superResponse);
  return [...bit, ...data, lightBitmap, luminance, mode, speed, sleepDelay, staticColor];
};

// 单键RGB设置
export const SingleRGBDataPack = (param: IKRGBDesc): number[] => {
  const { key, r, g, b } = param;
  const data: number[] = [key];
  if (r || r === 0) data.push(r);
  if (g || g === 0) data.push(g);
  if (b || b === 0) data.push(b);
  return data;
};

// TODO: RGBPack的协议有点问题
export const RGBDataPack = (KRGBDescs: KRGBDescs): number[] => {
  const data: number[] = [];
  KRGBDescs.forEach((item: IKRGBDesc) => {
    const { key, r, g, b } = item;
    data.push(key, r, g, b);
  });

  return data;
};

// 高级键

// DKS
export const DKSDataPack = (param: IDKSMode, v: string = '1.0.5'): number[] => {
  const { key, dks, trps, dbs } = param;
  const dbbits: number[] = [];
  for (let i = 0; i < dbs.length; i++) {
    dbbits.push(...computeHighLowByte(dbs[i]));
  }
  if (v === '1.0.5') {
    const dksbits: number[] = [];
    for (let i = 0; i < dks.length; i++) {
      dksbits.push(...computeHighLowByte(dks[i]));
    }
    return [key, ...dksbits, ...trps, ...dbbits];
  }
  return [key, ...dks, ...trps, ...dbbits];
};

// MPT
export const MPTDataPack = (param: IMPTMode): number[] => {
  const dbbits: number[] = [];
  const dksbits: number[] = [];
  const { key, dks, dbs } = param;
  for (let i = 0; i < dks.length; i++) {
    dksbits.push(...computeHighLowByte(dks[i]));
  }
  for (let i = 0; i < dbs.length; i++) {
    dbbits.push(...computeHighLowByte(preciseCalculate('multiply', dbs[i], 1000)));
  }
  return [key, ...dksbits, ...dbbits];
};
// MT
export const MTDataPack = (param: IMTMode, v: string = '1.0.5'): number[] => {
  const { key, dks, delay } = param;
  if (v === '1.0.5') {
    const dbbits: number[] = [];
    dks.forEach((dks: number) => {
      dbbits.push(...computeHighLowByte(dks));
    });
    return [key, ...dbbits, delay];
  }
  return [key, ...dks, delay];
};

// TGL
export const TGLDataPack = (param: ITGLMode, v: string = '1.0.5'): number[] => {
  const { key, dks, delay } = param;
  const greaterOrEqual105Tag = compareVersions(v, '1.0.5');
  const isGreaterOrEqual105Tag = ['greater', 'equal'].includes(greaterOrEqual105Tag);
  if (isGreaterOrEqual105Tag) {
    const dksbits = computeHighLowByte(dks);
    return [key, ...dksbits, delay / 10];
  }
  return [key, dks, delay];
};

// END
export const ENDDataPack = (param: IEndMode, v: string = '1.0.5'): number[] => {
  const { key, dks, delay } = param;
  const greaterOrEqual107Tag = compareVersions(v, '1.0.7');
  const greaterOrEqual105Tag = compareVersions(v, '1.0.5');
  const isGreaterOrEqual107Tag = ['greater', 'equal'].includes(greaterOrEqual107Tag);
  const isGreaterOrEqual105Tag = ['greater', 'equal'].includes(greaterOrEqual105Tag);

  if (isGreaterOrEqual107Tag) {
    const dksbits = computeHighLowByte(dks);
    const delaybits = computeHighLowByte(delay);
    return [key, ...dksbits, ...delaybits];
  }
  if (isGreaterOrEqual105Tag) {
    const dksbits = computeHighLowByte(dks);
    return [key, ...dksbits];
  }
  return [key, dks];
};

// SOCD
export const SOCDPack = (
  param: ISOCDMode | ISOCDModeV2 | ISOCDModeV3 | number,
  v: string = '1.0.5',
  read: boolean = false,
): number[] => {
  if (read) {
    return [param as number];
  }
  const greaterOrEqual107Tag = compareVersions(v, '1.0.7');
  const isGreaterOrEqual107Tag = ['greater', 'equal'].includes(greaterOrEqual107Tag);
  if (['1.0.5', '1.0.6'].includes(v) || isGreaterOrEqual107Tag) {
    const { pos1, pos2, key1, key2, type, mode, delay } = param as ISOCDModeV3;
    const key1Value = computeHighLowByte(key1);
    const key2Value = computeHighLowByte(key2);
    if (isGreaterOrEqual107Tag) {
      const delayValue = computeHighLowByte(delay);
      return [pos1, pos2, ...key1Value, ...key2Value, type, mode, ...delayValue];
    }
    return [pos1, pos2, ...key1Value, ...key2Value, type, mode];
  }
  const { key, dks1, mode1, mode2 } = param as ISOCDMode;
  return [key, dks1, mode1, dks1, key, mode2];
};
// RS
export const RSModePack = (param: IRSMode): number[] => {
  const { key, dks } = param;
  return [key, dks, dks, key];
};

export const MacroDataPack = (
  offset: number,
  count: number,
  keyValues: Uint16Array,
  keyStatus: number[],
  delays: Uint32Array,
): number[] => {
  const offsetValue = computeHighLowByte(offset);
  // 偏移地址

  const keys = [];

  for (let i = 0; i < keyValues.length; i++) {
    const keyValue = computeHighLowByte(keyValues[i]);
    keys.push(...keyValue);
    let data = 0;

    let prefix = 0x1;
    if (keyStatus[i] === 0) prefix = 8;

    // 将前4位赋值到高位
    data |= prefix << 24; // prefix 左移12位，占据前4位

    // 将后12位赋值到低位
    data |= delays[i] & 0x00ffffff; // 确保只保留低12位

    keys.push(...computeHighLowByte(data));
    keys.push(highByte16(data), highByte24(data));
    // pack[len++] = data & 0xff; // 最低字节 (Least Significant Byte - LSB)
    // pack[len++] = (data >> 8) & 0xff; // 第二个字节
    // pack[len++] = (data >> 16) & 0xff; // 第三个字节
    // pack[len++] = (data >> 24) & 0xff; // 最高字节 (Most Significant Byte - MSB)
  }

  // pack[1] = len - 4;
  // pack[3] = ComputeCheckSum(pack);

  return [...offsetValue, count, ...keys];
  // return [key, index, len, mode, num, delay];
};
export const MacroModePack = (
  key: number,
  index: number,
  macroLen: number,
  mode: number,
  num: number,
  delay: number,
): number[] => {
  return [
    key,
    ...computeHighLowByte(index),
    macroLen,
    mode,
    ...computeHighLowByte(num),
    ...computeHighLowByte(delay),
    highByte16(delay),
  ];
};

export const RM6X21Pack = (matrix6x21: number, datatype: number): number[] => {
  return [matrix6x21, datatype, ...getSomeBits(2, 0xff)];
};

export const SIGNPack = (unlock: number, data: number[], sn: number[]): number[] => {
  const placeholderBit10 = getSomeBits(1, 0x10);
  const placeholderBitff = getSomeBits(4, 0xff);
  return [unlock, ...placeholderBit10, ...data, ...placeholderBit10, ...sn, ...placeholderBitff];
};

export const ERASEPack = (size: number): number[] => {
  const bit10 = getSomeBits(4);
  const bit = computeHighLowByte(size);
  const bit16 = highByte16(size);
  const bit24 = highByte24(size);
  const bitff = getSomeBits(1, 0xff);
  return [...bit10, ...bit, bit16, bit24, ...bitff];
};

export const REBOOTPack = (): number[] => {
  return [...getSomeBits(2, 0xff)];
};

export const TOAPPPack = (size: number, crc: number): number[] => {
  const bit10 = getSomeBits(4);
  const bit = computeHighLowByte(size);
  const bit16 = highByte16(size);
  const bit24 = highByte24(size);
  const bitCrc = computeHighLowByte(crc);
  return [...bit10, ...bit, bit16, bit24, ...bitCrc];
};

export const WRITEPack = (param: IWriteParam): number[] => {
  const { addr, size, codes } = param;
  // 244 = 256 - 8(addrBit,addrBit16,addrBit24,sizeBit) - 4 (head)
  const addrBit = computeHighLowByte(addr);
  const addrBit16 = highByte16(addr);
  const addrBit24 = highByte24(addr);
  const sizeBit = computeHighLowByte(size);
  const bit244ff = getSomeBits(244, 0xff);

  codes.forEach((code, index) => {
    bit244ff[index] = code;
  });

  return [...addrBit, addrBit16, addrBit24, ...sizeBit, ...bit244ff];
};

export const RCRCPack = (size: number): number[] => {
  const bit10 = getSomeBits(4);
  const bit = computeHighLowByte(size);
  const bit16 = highByte16(size);
  const bit24 = highByte24(size);
  const bitff = getSomeBits(2, 0xff);
  return [...bit10, ...bit, bit16, bit24, ...bitff];
};

export const PICSTARTPack = (size: number, picId: number) => {
  const pack = computeHighLowByte(size);
  const b16 = highByte16(size);
  const b24 = highByte24(size);
  const bitff = getSomeBits(2, 0xff);
  return [size, ...pack, b16, b24, picId, ...bitff];
};

export const PICWRITEPack = (addr: number, size: number, data: number[]): number[] => {
  const addrPack = computeHighLowByte(addr);
  const addPb16 = highByte16(addr);
  const addPb24 = highByte24(addr);
  const sizePack = computeHighLowByte(size);
  const dataPack = [];

  for (let index = 0; index < 244; index++) {
    dataPack.push(data[index]);
  }

  return [...addrPack, addPb16, addPb24, ...sizePack, ...sizePack];
};

export const saturationDataPack = (saturationDescs: number[]): number[] => {
  const ff = getSomeBits(2, 0xff);
  return [0x44, ...saturationDescs, ...ff];
};
