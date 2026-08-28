import { Protocol } from '../constants/byte';
import { bitReadWrite, createProtocol } from '../utils/index';
import { DBDataPack, RM6X21Pack } from '../utils/pack';

const { KB2_CMD_DB, KB2_CMD_RM6X21 } = Protocol;
import { IDB } from '../types/interface';
import {
  getAxisRecdata,
  getCmdRecdata,
  getDksTravelRecdata,
  getDpDrRecdata,
  getGlobalTouchTravelRecdata,
  getRm6X21Recdata,
  getRtTravelRecdata,
  getSingleTravelRecdata,
} from '../utils/recdata';

export class PerformanceController {
  cmdDB(isrw: boolean, param?: IDB): Uint8Array {
    const rw = bitReadWrite(isrw);
    const db = DBDataPack(param);
    const data = [rw, ...db];
    const len = data.length;
    const cmd = KB2_CMD_DB;
    return createProtocol(len, cmd, data);
  }

  rm6X21Pack(matrix6x21: number, datatype: number): Uint8Array {
    const data = RM6X21Pack(matrix6x21, datatype);
    const len = data.length;
    const cmd = KB2_CMD_RM6X21;
    return createProtocol(len, cmd, data);
  }

  getRm6X21data(data: Uint8Array) {
    return getRm6X21Recdata(data);
  }

  getGlobalTouchTravel(data: Uint8Array) {
    return getGlobalTouchTravelRecdata(data);
  }
  /**
   * @method getSingleTravel
   * @desc 获取单键行程
   * @param data 参数名 Uint8Array
   * @return {Protocol} Protocol
   * @example
   */

  getSingleTravel(data: Uint8Array, decimal: number) {
    return getSingleTravelRecdata(data, decimal);
  }

  /**
   * @method getDksTravel
   * @desc 获取DKS行程
   * @param data 参数名 Uint8Array
   * @return {Protocol} Protocol
   * @example
   */

  getDksTravel(data: Uint8Array) {
    return getDksTravelRecdata(data);
  }

  /**
   * @method getRtTravel
   * @desc 获取RT行程
   * @param data 参数名 Uint8Array
   * @return {Protocol} Protocol
   * @example
   */

  getRtTravel(data: Uint8Array) {
    return getRtTravelRecdata(data);
  }

  /**
   * @method getDpDr
   * @desc 获取死区
   * @param data 参数名 Uint8Array
   * @return {Protocol} Protocol
   * @example
   */

  getDpDr(data: Uint8Array) {
    return getDpDrRecdata(data);
  }
  /**
   * @method getAxis
   * @desc 获取轴体
   * @param data 参数名 Uint8Array
   * @return {Protocol} Protocol
   * @example
   */

  getAxis(data: Uint8Array) {
    return getAxisRecdata(data);
  }

  getAxisList(data: Uint8Array) {
    return getCmdRecdata(data);
  }
}
