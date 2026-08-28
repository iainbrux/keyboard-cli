import { DefKeyValue, IKey, Keys } from '../types/interface';
export declare class KeyController {
    cmdDefKey(row1: number, row2: number): Uint8Array;
    getDefKey(data: Uint8Array): DefKeyValue;
    cmdKey(isrw: boolean, param: Keys): Uint8Array;
    cmdLayout(isrw: boolean, param: IKey): Uint8Array;
    getLayoutModel(data: Uint8Array): {
        touchMode: string;
        advancedKeyMode: number;
    };
    getFnLayoutKeyRecdata(data: Uint8Array): Keys;
}
