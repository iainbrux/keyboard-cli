import { Protocol } from '../constants/byte';
import { createProtocol, createProtocolSlice } from '../utils/index';
import {
  ERASEPack,
  PICSTARTPack,
  PICWRITEPack,
  RCRCPack,
  REBOOTPack,
  SIGNPack,
  TOAPPPack,
  WRITEPack,
} from '../utils/pack';
import { getCrcRecdata, getSignRecdata, getWriteRecdata } from '../utils/recdata';

const {
  KB2_BL_ERASE,
  KB2_BL_REBOOT,
  KB2_BL_TOAPP,
  KB2_BL_WRITE,
  KB2_BL_RCRC,
  KB2_BL_SIGN,
  KB2_CMD_PIC,
  KB2_PIC_WRITE,
} = Protocol;
import { IWriteParam } from '../types/interface';

export class SystemController {
  blSIGN(unlock: number, data: any, sn: number[]): Uint8Array {
    const sign = SIGNPack(unlock, data, sn);
    const len = sign.length;
    const cmd = KB2_BL_SIGN;
    return createProtocol(len, cmd, sign);
  }

  blERASE(size: number): Uint8Array {
    const data = ERASEPack(size);
    const len = data.length;
    const cmd = KB2_BL_ERASE;
    return createProtocol(len, cmd, data);
  }

  blREBOOT(): Uint8Array {
    const data = REBOOTPack();
    const len = data.length;
    const cmd = KB2_BL_REBOOT;
    return createProtocol(len, cmd, data);
  }

  blTOAPP(size: number, crc: number): Uint8Array {
    const data = TOAPPPack(size, crc);
    const len = data.length;
    const cmd = KB2_BL_TOAPP;
    return createProtocol(len, cmd, data);
  }

  blWRITE(param: IWriteParam): Uint8Array[] {
    const data = WRITEPack(param);
    const len = data.length;
    const cmd = KB2_BL_WRITE;
    return createProtocolSlice(len, cmd, data);
  }

  blRCRC(size: number): Uint8Array {
    const data = RCRCPack(size);
    const len = data.length;
    const cmd = KB2_BL_RCRC;
    return createProtocol(len, cmd, data);
  }

  // 图片设置
  picStart(size: number, picId: number): Uint8Array {
    const data = PICSTARTPack(size, picId);
    const len = data.length;
    const cmd = KB2_CMD_PIC;
    return createProtocol(len, cmd, data);
  }

  // 图片写入
  picWrite(addr: number, size: number, value: number[]): Uint8Array[] {
    const data = PICWRITEPack(addr, size, value);
    const len = data.length;
    const cmd = KB2_PIC_WRITE;
    return createProtocolSlice(len, cmd, data);
  }

  getSignature(data: Uint8Array) {
    return getSignRecdata(data);
  }

  getWrite(data: Uint8Array) {
    return getWriteRecdata(data);
  }

  getCrc(data: Uint8Array) {
    return getCrcRecdata(data);
  }
}
