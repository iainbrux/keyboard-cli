import { IDKSMode, IEndMode, IMPTMode, IMTMode, IRSMode, ISOCDMode, ISOCDModeV2, ISOCDModeV3, ITGLMode } from '../types/interface';
export declare class HigherKeyController {
    getTrps(data: Uint8Array): {
        trps: number;
    };
    cmdDKS(isrw: boolean, param?: IDKSMode): Uint8Array;
    getDks(data: Uint8Array): {
        dks: number;
    };
    getMtorTgl(data: Uint8Array): number;
    cmdMPT(isrw: boolean, param: IMPTMode): Uint8Array;
    getMptData(data: Uint8Array): {
        dks: number[];
        dbs: number[];
    };
    cmdMT(isrw: boolean, param: IMTMode): Uint8Array;
    getMtRecdata(data: Uint8Array): Uint8Array;
    cmdTGL(isrw: boolean, param: ITGLMode): Uint8Array;
    getTglData(data: Uint8Array): {
        dks: number;
        delay: number;
    };
    cmdEND(isrw: boolean, param: IEndMode, v?: string): Uint8Array;
    getEndData(data: Uint8Array): {
        dks: number;
        delay: number;
    };
    cmdSOCD(isrw: boolean, param?: ISOCDMode | ISOCDModeV2 | ISOCDModeV3 | number, v?: string): Uint8Array;
    getSocdData(data: Uint8Array, v?: string): {
        pos1: number;
        pos2: number;
        key1: number;
        key2: number;
        type: number;
        mode: number;
        delay: number;
        pos?: undefined;
        key?: undefined;
    } | {
        pos: number;
        key: number;
        type: number;
        mode: number;
        pos1?: undefined;
        pos2?: undefined;
        key1?: undefined;
        key2?: undefined;
        delay?: undefined;
    };
    cmdRS(isrw: boolean, param: IRSMode): Uint8Array;
    getRsData(data: Uint8Array): {
        dks1: number;
        dks2: number;
    };
    cmdMacro(isrw: boolean, offset: number, count: number, keyValues: Uint16Array, keyStatus: number[], delays: Uint32Array): Uint8Array;
    modeMacro(isrw: boolean, key: number, index?: number, macroLen?: number, mode?: number, num?: number, delay?: number): Uint8Array;
    getModeMacro(data: Uint8Array): {
        key: number;
        id: number;
        len: number;
        mode: number;
        num: number;
        delay: number;
    };
}
