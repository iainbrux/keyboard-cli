import { Protocol } from '../constants/byte';
import { bitReadWrite, createProtocol } from '../utils/index';
import {
  DKSDataPack,
  ENDDataPack,
  MacroDataPack,
  MacroModePack,
  MPTDataPack,
  MTDataPack,
  RSModePack,
  SOCDPack,
  TGLDataPack,
} from '../utils/pack';

const {
  KB2_CMD_DKS,
  KB2_CMD_MPT,
  KB2_CMD_MT,
  KB2_CMD_TGL,
  KB2_CMD_END,
  KB2_CMD_SOCD,
  KB2_CMD_RS,
  KB2_CMD_MACRO,
  KB2_CMD_MACRO_MODE,
} = Protocol;
import {
  IDKSMode,
  IEndMode,
  IMPTMode,
  IMTMode,
  IRSMode,
  ISOCDMode,
  ISOCDModeV2,
  ISOCDModeV3,
  ITGLMode,
} from '../types/interface';
import {
  getDksRecdata,
  getEndRecdata,
  getMacroRecdata,
  getMptRecdata,
  getMtorTglRecdata,
  getMTRecdata,
  getRsRecdata,
  getSocdRecdata,
  getTglRecdata,
  getTrpsRecdata,
} from '../utils/recdata';

export class HigherKeyController {
  getTrps(data: Uint8Array) {
    return getTrpsRecdata(data);
  }
  /**
   * @method cmdDKS
   * @desc DKS模式
   * @param {Boolean} isrw 读写标识
   * @param {Object} DKSMode DKSMode
   * @return {Protocol} Protocol
   * @example
   */

  cmdDKS(isrw: boolean, param?: IDKSMode): Uint8Array {
    const dksData = DKSDataPack(param);
    const rw = bitReadWrite(isrw);
    const data = [rw, ...dksData];
    const len = data.length;
    const cmd = KB2_CMD_DKS;
    return createProtocol(len, cmd, data);
  }

  getDks(data: Uint8Array) {
    return getDksRecdata(data);
  }

  getMtorTgl(data: Uint8Array) {
    return getMtorTglRecdata(data);
  }

  /**
   * @method cmdMPT
   * @desc MPT模式
   * @param {Boolean} isrw 读写标识
   * @param {Object} MPTMode MPTMode
   * @return {Protocol} Protocol
   * @example
   */
  cmdMPT(isrw: boolean, param: IMPTMode): Uint8Array {
    const mptData = isrw ? MPTDataPack({ key: param.key, dks: [], dbs: [] }) : MPTDataPack(param);
    const rw = bitReadWrite(isrw);
    const data = [rw, ...mptData];
    const len = data.length;
    const cmd = KB2_CMD_MPT;
    return createProtocol(len, cmd, data);
  }

  getMptData(data: Uint8Array) {
    return getMptRecdata(data);
  }

  /**
   * @method cmdMT
   * @desc MT模式
   * @param {Boolean} isrw 读写标识
   * @param {Object} MPTMode MPTMode
   * @return {Protocol} Protocol
   * @example
   */
  cmdMT(isrw: boolean, param: IMTMode): Uint8Array {
    const mtData = MTDataPack(param);
    const rw = bitReadWrite(isrw);
    const data = [rw, ...mtData];
    const len = data.length;
    const cmd = KB2_CMD_MT;
    return createProtocol(len, cmd, data);
  }

  getMtRecdata(data: Uint8Array) {
    return getMTRecdata(data);
  }

  /**
   * @method cmdTGL
   * @desc TGL模式
   * @param {Boolean} isrw 读写标识
   * @param {Object} TGLMode TGLMode
   * @return {Protocol} Protocol
   * @example
   */

  cmdTGL(isrw: boolean, param: ITGLMode): Uint8Array {
    const tglData = isrw ? TGLDataPack({ key: param.key, dks: 0, delay: 0 }) : TGLDataPack(param);
    const rw = bitReadWrite(isrw);
    const data = [rw, ...tglData];
    const len = data.length;
    const cmd = KB2_CMD_TGL;
    return createProtocol(len, cmd, data);
  }

  getTglData(data: Uint8Array) {
    return getTglRecdata(data);
  }

  /**
   * @method cmdEND
   * @desc END模式
   * @param {Boolean} isrw 读写标识
   * @param {Object} EndMode EndMode
   * @return {Protocol} Protocol
   * @example
   */

  cmdEND(isrw: boolean, param: IEndMode, v: string = '1.0.5'): Uint8Array {
    const endData = isrw ? ENDDataPack({ key: param.key, dks: 0, delay: 0 }) : ENDDataPack(param, v);
    const rw = bitReadWrite(isrw);
    const data = [rw, ...endData];
    const len = data.length;
    const cmd = KB2_CMD_END;
    return createProtocol(len, cmd, data);
  }

  getEndData(data: Uint8Array) {
    return getEndRecdata(data);
  }

  /**
   * @method cmdSOCD
   * @desc SOCD模式
   * @param {Boolean} isrw 读写标识
   * @param {Object} SOCDMode SOCDMode
   * @return {Protocol} Protocol
   * @example
   */

  cmdSOCD(isrw: boolean, param?: ISOCDMode | ISOCDModeV2 | ISOCDModeV3 | number, v: string = '1.0.5'): Uint8Array {
    const socd = isrw ? SOCDPack(param, v, true) : SOCDPack(param, v);
    const rw = bitReadWrite(isrw);
    const data = [rw, ...socd];
    const len = data.length;
    return createProtocol(len, KB2_CMD_SOCD, data);
  }

  getSocdData(data: Uint8Array, v: string = '1.0.5') {
    return getSocdRecdata(data, v);
  }

  /**
   * @method cmdRS
   * @desc RS模式
   * @param {Boolean} isrw 读写标识
   * @param {Object} RSMode RSMode
   * @return {Protocol} Protocol
   * @example
   */

  cmdRS(isrw: boolean, param: IRSMode): Uint8Array {
    const rs = RSModePack(param);
    console.log('rs', rs);
    const rw = bitReadWrite(isrw);
    const data = [rw, ...rs];
    const len = data.length;
    const cmd = KB2_CMD_RS;
    return createProtocol(len, cmd, data);
  }

  getRsData(data: Uint8Array) {
    return getRsRecdata(data);
  }

  /**
   * @method cmdMacro
   * @desc Macro模式
   * @param {Boolean} isrw 读写标识
   * @param {Object} MacroMode MacroMode
   * @return {Protocol} Protocol
   * @example
   */

  cmdMacro(
    isrw: boolean,
    offset: number,
    count: number,
    keyValues: Uint16Array,
    keyStatus: number[],
    delays: Uint32Array,
  ): Uint8Array {
    const rw = bitReadWrite(isrw);
    const macro = MacroDataPack(offset, count, keyValues, keyStatus, delays);
    const data = [rw, ...macro];
    const len = data.length;
    const cmd = KB2_CMD_MACRO;
    return createProtocol(len, cmd, data);
  }

  modeMacro(
    isrw: boolean,
    key: number,
    index?: number,
    macroLen?: number,
    mode?: number,
    num?: number,
    delay?: number,
  ): Uint8Array {
    const rw = bitReadWrite(isrw);
    const macroMode = MacroModePack(key, index, macroLen, mode, num, delay);
    const data = [rw, ...macroMode];
    const len = data.length;
    const cmd = KB2_CMD_MACRO_MODE;
    return createProtocol(len, cmd, data);
  }

  getModeMacro(data: Uint8Array) {
    return getMacroRecdata(data);
  }
}
