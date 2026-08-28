import { Protocol } from '../constants/byte';
import { bitReadWrite, createProtocol } from '../utils/index';
import { PRGBDatapack, RGBDataPack, saturationDataPack, SingleRGBDataPack, SRGBDatapack } from '../utils/pack';

const { KB2_CMD_LOGORGB, KB2_CMD_PRGB, KB2_CMD_KRGB, KB2_CMD } = Protocol;
import { IKRGBDesc, ILightMode, KRGBDescs, VersionString } from '../types/interface';
import { getPRGBRecdata, getSingleRGBRecdata, getSpecialPRGBRecdata } from '../utils/recdata';

export class LightingController {
  /**
   * @method cmdPRGB
   * @desc 表示灯光设置RGB
   * @param {Boolean} isrw 读写标识
   * @param {Object} 参数名 LightMode
   * @return {Protocol} Protocol
   * @example
   *
   */

  cmdPRGB(isrw: boolean, param?: ILightMode, version: VersionString = '1.0.7'): Uint8Array {
    const protocols = this.RGB(isrw, param, KB2_CMD_PRGB, version);
    return protocols;
  }

  /**
   * @method cmdSRGB
   * @desc 特殊的灯光设置
   * @param {Boolean} isrw 读写标识
   * @param {Object} 参数名 LightMode
   * @return {Protocol} Protocol
   */
  cmdSRGB(isrw: boolean, param?: ILightMode): Uint8Array {
    const protocols = this.SRGB(isrw, param);
    return protocols;
  }
  /**
   * @method getPRGB
   * @desc 表示灯光设置RGB
   * @param data 参数名 Uint8Array
   * @return {Protocol} Protocol
   * @example
   *
   */

  getPRGB(data: Uint8Array): ILightMode {
    const lightMode = getPRGBRecdata(data);
    return lightMode;
  }

  /**
   * @method cmdLogoRGB
   * @desc 表示logo灯光设置RGB
   * @param {Boolean} isrw 读写标识
   * @param {Object} LightMode 灯光对象
   * @return {Protocol} Protocol
   * @example
   *
   */

  cmdLogoRGB(isrw: boolean, param: ILightMode): Uint8Array {
    const protocols = this.RGB(isrw, param, KB2_CMD_LOGORGB);
    return protocols;
  }

  private RGB(
    isrw: boolean,
    param: ILightMode,
    cmd: Protocol = KB2_CMD_PRGB,
    version: VersionString = '1.0.7',
  ): Uint8Array {
    const rw = bitReadWrite(isrw);
    const prgbData = PRGBDatapack(param, version);
    const data = [rw, ...prgbData];
    const len = data.length;
    return createProtocol(len, cmd, data);
  }

  private SRGB(isrw: boolean, param: ILightMode, cmd: Protocol = KB2_CMD_PRGB): Uint8Array {
    const rw = bitReadWrite(isrw);
    const prgbData = SRGBDatapack(param);
    const data = [rw, ...prgbData];
    const len = data.length;
    return createProtocol(len, cmd, data);
  }
  /**
   * @method cmdSingleRGB
   * @desc 单键RGB设置
   * @param {Boolean} isrw 读写标识
   * @param {Object} param IKRGBDesc
   * @return {Protocol} Protocol
   * @example
   */

  cmdSingleRGB(isrw: boolean, param: IKRGBDesc): Uint8Array {
    const rw = bitReadWrite(isrw);
    const rgb = SingleRGBDataPack(param);
    const data = [rw, ...rgb];
    const len = data.length;
    const cmd = KB2_CMD_KRGB;
    return createProtocol(len, cmd, [...data]);
  }

  getSingleRGB(data: Uint8Array): IKRGBDesc {
    return getSingleRGBRecdata(data);
  }

  /**
   * @method cmdKRGB
   * @desc 自定义模式下的定义
   * @param {Boolean} isrw 读写标识
   * @param {Object} KRGBDesc KRGBDesc
   * @return {Protocol} Protocol
   * @example
   */

  cmdKRGB(isrw: boolean, krgbDescs: KRGBDescs): Uint8Array {
    const rw = bitReadWrite(isrw);
    const rgb = RGBDataPack(krgbDescs);
    // 59 = 64 - 5(keys)
    const complete = 59 - rgb.length;
    const completeFF = new Uint8Array(complete).fill(0xff);
    const data = [rw, ...rgb];
    const len = data.length;
    const protocol: number[] = Array.from(createProtocol(len, KB2_CMD_KRGB, data));
    protocol.splice(len + 4, completeFF.length);
    return new Uint8Array([...protocol, ...completeFF]);
  }

  /**
   * @method cmdKRGB
   * @desc 特殊的灯光处理
   * @param {Uint8Array} data KRGBDesc
   * @return {ILightMode} ILightMode
   * @example
   */

  getSpecialSingleRGB(data: Uint8Array): ILightMode {
    return getSpecialPRGBRecdata(data);
  }

  cmdRGBSaturation(isrw: boolean, param: number[]): Uint8Array {
    const rgb = saturationDataPack(param);
    // console.log('cmdRGBSaturation rgb: ', rgb);
    const data = [...rgb];
    // console.log('cmdRGBSaturation data: ', data);
    const len = data.length;
    const cmd = KB2_CMD;
    return createProtocol(len, cmd, data);
  }
}
