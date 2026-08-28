import { Head as HeadByte } from '../constants/byte';

// 读写位
export const bitReadWrite = (value: boolean = true): number => {
  const r = 0x00;
  const w = 0x01;
  return value ? r : w;
};

// 灯光的bitmap
export const getLightBitmap = (lightSwitch: boolean, reverseEffect: boolean, superResponse: boolean): number => {
  // 初始化结果值为0
  let Bitmap = 0;

  // 设置灯光开关（第0位）
  if (lightSwitch) Bitmap |= 0x01 << 0;

  // 设置动态灯效方向（第1位）
  if (reverseEffect) Bitmap |= 0x01 << 1;

  // 设置超级响应开关（第4位）
  if (superResponse) Bitmap |= 0x01 << 4;

  return Bitmap;
};

// 默认得到一个指定长度的数组，元素为指定的位
export const getSomeBits = (num: number = 1, bit: number = 0x00): number[] => Array(num).fill(bit);

// 低字节
export const lowByte = (value: number): number => value & 0xff;

// 高字节
export const highByte = (value: number): number => (value >> 8) & 0xff;

// 高字节16
export const highByte16 = (value: number): number => (value >> 16) & 0xff;

// 高字节24
export const highByte24 = (value: number): number => (value >> 24) & 0xff;

// 计算高低字节
export const computeHighLowByte = (value: number): number[] => [lowByte(value), highByte(value)];

// Head
export const computeHead = (len: number, cmd: number, crc: number): number[] => {
  return [HeadByte, len, cmd, crc];
};

// CRC 计算方法
export const computeCRC = (len: number, cmd: number, data: number[]): number => {
  let crc = 0x35;
  crc += HeadByte;
  crc += len;
  crc += cmd;
  if (len > 0 && len <= 63 * 4) {
    crc += data[data.length - 1];
  }
  return crc;
};

// 协议 计算协议数据格式
export const computeProtocol = (head: number[], data: number[], len: number = 64): Uint8Array => {
  const cmd = [...head, ...data];
  const protocols = new Uint8Array(len).fill(0);
  cmd.forEach((val, idx) => {
    protocols[idx] = val;
  });
  return protocols;
};

export const computeProtocolSlice = (head: number[], data: number[], len: number = 64): Uint8Array[] => {
  // 截取长度64的数组得到一个二维数组
  const cmd = [...head, ...data];
  const arr: Uint8Array[] = [];
  for (let index = 0; index < cmd.length; index += len) {
    const protocols = new Uint8Array(len).fill(0);
    cmd.slice(index, index + len).forEach((val, idx) => {
      protocols[idx] = val;
    });
    arr.push(protocols);
  }
  return arr;
};

// 创建协议数据
export const createProtocol = (len: number, cmd: number, data: number[]): Uint8Array => {
  const crc = computeCRC(len, cmd, data);
  const head = computeHead(len, cmd, crc);
  const protocol = computeProtocol(head, data);
  return protocol;
};

// 创建协议数据
export const createProtocolSlice = (len: number, cmd: number, data: number[]): Uint8Array[] => {
  const crc = computeCRC(len, cmd, data);
  const head = computeHead(len, cmd, crc);
  const protocol = computeProtocolSlice(head, data);
  return protocol;
};

// 比较版本号
export const compareVersions = (version1: string, version2: string): string => {
  // 将版本号字符串分割成数组
  const v1Parts = version1.split('.').map(Number);
  const v2Parts = version2.split('.').map(Number);

  // 获取最长的数组长度
  const maxLength = Math.max(v1Parts.length, v2Parts.length);

  // 补齐两个数组的长度（用0填充）
  while (v1Parts.length < maxLength) v1Parts.push(0);
  while (v2Parts.length < maxLength) v2Parts.push(0);

  // 逐位比较
  for (let i = 0; i < maxLength; i++) {
    if (v1Parts[i] > v2Parts[i]) return 'greater';
    if (v1Parts[i] < v2Parts[i]) return 'less';
  }

  return 'equal';
};

export const computeCheckSum = (pack: number[]) => {
  let checkSum = 0x35;
  const len = pack[1];

  checkSum += pack[0];
  checkSum += len;
  checkSum += pack[2];

  if (len > 0 && len <= 63 * 4) {
    checkSum += pack[len + 4 - 1];
  }

  return checkSum;
};

export * from './decimal';
