import { DefKeyValue, IDB, ILightMode, Keys } from '../types/interface';
export declare const getCmdSyncRecdata: (data: Uint8Array) => {
    BoardID: number;
    KeyboardLayout: number;
    KeyType: number;
    CustomerID: number;
    ProductionId: number;
    KeyboardRunMode: number;
    KeyboardSN: string;
    firewareSpaceSize: number;
    appVersion: string;
    appBuildDate: string;
    versionString: string;
};
export declare const getCmdRecdata: (data: Uint8Array) => string | number | boolean | {
    precision: number;
    decimalPlace: number;
    minTouchTravel: number;
    maxTouchTravel: number;
    VID: number;
    PID: number;
} | {
    currentSystem: string;
    hasWinMode: boolean;
} | {
    currentSystem: string;
    hasMacMode: boolean;
} | {
    r: number;
    g: number;
    b: number;
} | {
    configID: number;
    hasFourConfig: boolean;
    hasAxisSetting?: undefined;
    axisList?: undefined;
} | {
    hasAxisSetting: boolean;
    axisList: number[];
    configID?: undefined;
    hasFourConfig?: undefined;
};
export declare const getPRGBRecdata: (data: Uint8Array) => ILightMode;
export declare const getSpecialPRGBRecdata: (data: Uint8Array) => ILightMode;
export declare const getSingleRGBRecdata: (data: Uint8Array) => {
    key: number;
    R: number;
    G: number;
    B: number;
};
export declare const getDefKeyRecdata: (data: Uint8Array) => DefKeyValue;
export declare const getFnLayoutKeyRecdata: (data: Uint8Array) => Keys;
export declare const getGlobalTouchTravelRecdata: (data: Uint8Array) => IDB;
export declare const getLayoutModelRecdata: (data: Uint8Array) => {
    touchMode: string;
    advancedKeyMode: number;
};
export declare const getSingleTravelRecdata: (data: Uint8Array, decimal: number) => string;
export declare const getDksTravelRecdata: (data: Uint8Array) => number;
export declare const getRtTravelRecdata: (data: Uint8Array) => number;
export declare const getDpDrRecdata: (data: Uint8Array) => number;
export declare const getAxisRecdata: (data: Uint8Array) => {
    axis: number;
};
export declare const getMTRecdata: (data: Uint8Array) => Uint8Array;
export declare const getTrpsRecdata: (data: Uint8Array) => {
    trps: number;
};
export declare const getMtorTglRecdata: (data: Uint8Array) => number;
export declare const getDksRecdata: (data: Uint8Array) => {
    dks: number;
};
export declare const getMptRecdata: (data: Uint8Array) => {
    dks: number[];
    dbs: number[];
};
export declare const getMtRecdata: (data: Uint8Array) => Uint8Array;
export declare const getTglRecdata: (data: Uint8Array) => {
    dks: number;
    delay: number;
};
export declare const getEndRecdata: (data: Uint8Array) => {
    dks: number;
    delay: number;
};
export declare const getSocdRecdata: (data: Uint8Array, v?: string) => {
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
export declare const getRsRecdata: (data: Uint8Array) => {
    dks1: number;
    dks2: number;
};
export declare const getMacroRecdata: (data: Uint8Array) => {
    key: number;
    id: number;
    len: number;
    mode: number;
    num: number;
    delay: number;
};
export declare const getMacroDataRecdata: (data: Uint8Array) => Uint8Array;
export declare const getPicRecdata: (data: Uint8Array) => {
    addrBuff: number;
    sizeBuff: number;
};
export declare const getRm6X21Recdata: (data: Uint8Array) => number[][];
export declare const getSignRecdata: (data: Uint8Array) => {
    signSuccess: boolean;
    signature: number[];
};
export declare const getWriteRecdata: (data: Uint8Array) => {
    currentUpdateAddress: number;
};
export declare const getCrcRecdata: (data: Uint8Array) => number;
