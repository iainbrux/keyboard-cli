import { Protocol } from '../constants/byte';
import { createProtocol } from '../utils/index';
import { CMDPack, SYNCPack } from '../utils/pack';

const { KB2_CMD, KB2_CMD_SYNC } = Protocol;
import { ICmd } from '../types/interface';
import { getCmdRecdata, getCmdSyncRecdata } from '../utils/recdata';

export class InfoController {
  /**
   * @method cmd
   * @desc 表示发送命令,让设备执行特定的动作。
   * @param {Object} 参数名 Cmd
   * @return {Protocol} Protocol
   * @example
   *
   */

  cmd(param: ICmd): Uint8Array {
    const data = CMDPack(param);
    const len = data.length;
    const cmd = KB2_CMD;
    return createProtocol(len, cmd, data);
  }

  getCmd(data: Uint8Array) {
    // 直接在这里解包
    return getCmdRecdata(data);
  }

  /**
   * @method cmdSync
   * @desc 表示返回关于设备的基础信息。
   * @return {Protocol} Protocol
   * @example
   *
   */
  cmdSync(): Uint8Array {
    const data = SYNCPack();
    const len = data.length;
    const cmd = KB2_CMD_SYNC;
    return createProtocol(len, cmd, data);
  }

  getCmdSync(data: Uint8Array) {
    return getCmdSyncRecdata(data);
  }
}
