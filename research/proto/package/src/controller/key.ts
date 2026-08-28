import { Protocol } from '../constants/byte';
import { bitReadWrite, createProtocol } from '../utils/index';
import { KeyDataPack, KeyLayoutDataPack } from '../utils/pack';
import { getDefKeyRecdata, getFnLayoutKeyRecdata, getLayoutModelRecdata } from '../utils/recdata';

const { KB2_CMD_DEFKEY, KB2_CMD_KEY } = Protocol;
import { DefKeyValue, IKey, Keys } from '../types/interface';

export class KeyController {
  /**
   * @method cmdDefKey
   * @desc 在打开驱动时会使用此命令读取布局。ROW表示行 一次传输2行，三次可传输全部按键。
   * @param {number} 参数名 row1
   * @param {number} 参数名 row2
   * @return {Protocol} Protocol
   * @example
   *
   */

  cmdDefKey(row1: number, row2: number): Uint8Array {
    const rw = bitReadWrite();
    const data = [rw, row1, row2];
    const len = data.length;
    const cmd = KB2_CMD_DEFKEY;
    return createProtocol(len, cmd, data);
  }
  /**
   * @method getDefKey
   * @desc 解CMD的包
   * @param {Uint8Array} 参数名 data
   * @return {Protocol} Protocol
   * @example
   *
   */

  getDefKey(data: Uint8Array): DefKeyValue {
    return getDefKeyRecdata(data);
  }

  /**
   * @method cmdKey
   * @desc 表示改建功能，查询键功能
   * @param {Boolean} isrw 读写标识
   * @param {Keys} param Keys
   * @return {Protocol} Protocol
   * @example
   */

  cmdKey(isrw: boolean, param: Keys): Uint8Array {
    const rw = bitReadWrite(isrw);
    const keyData = KeyDataPack(param);
    const data = [rw, ...keyData];
    const len = data.length;
    const cmd = KB2_CMD_KEY;
    return createProtocol(len, cmd, data);
  }

  /**
   * @method cmdLayout
   * @desc 表示改建功能，查询键功能
   * @param {Boolean} isrw 读写标识
   * @param {Key} param Key
   * @return {Protocol} Protocol
   * @example
   */

  cmdLayout(isrw: boolean, param: IKey): Uint8Array {
    const rw = bitReadWrite(isrw);
    const keyData = KeyLayoutDataPack(param);
    const data = [rw, ...keyData];
    const len = data.length;
    const cmd = KB2_CMD_KEY;
    return createProtocol(len, cmd, data);
  }

  // 解包
  getLayoutModel(data: Uint8Array) {
    return getLayoutModelRecdata(data);
  }

  getFnLayoutKeyRecdata(data: Uint8Array) {
    return getFnLayoutKeyRecdata(data);
  }
}
