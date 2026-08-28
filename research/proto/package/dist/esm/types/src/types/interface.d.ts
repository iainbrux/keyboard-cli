import { OrderType } from '../constants/param';
export interface IDefKeyInfo {
    keyValue: number;
    location: {
        row: number;
        col: number;
    };
}
export type DefKeyValue = [IDefKeyInfo[], IDefKeyInfo[]];
export interface IEnd {
    keys: number;
    dks: number;
}
export interface IKRGBDesc {
    key: number;
    r?: number;
    g?: number;
    b?: number;
}
export type KRGBDescs = IKRGBDesc[];
export type cmdType = keyof typeof OrderType;
export interface ICmd {
    type: cmdType;
    hArgs?: number[];
    is8bit?: boolean;
}
export interface IKey {
    key: number;
    layout: number;
    value?: number;
}
export type Keys = IKey[];
export type LightModeType = 'static' | 'custom' | 'dynamic';
export type LightLogoModeType = 'static' | 'dynamic';
export interface ILightMode {
    open: boolean;
    direction: boolean;
    superResponse: boolean;
    speed: number;
    colors: string[];
    mode: number;
    luminance: number;
    sleepDelay: number;
    staticColor: number;
    type?: LightModeType;
    dynamicColorId?: number;
}
export interface ILoGoLightMode {
    open: boolean;
    direction: boolean;
    superResponse: boolean;
    speed: number;
    colors: string[];
    mode: number;
    luminance: number;
    sleepDelay: number;
    staticColor: number;
    type?: LightLogoModeType;
}
export interface IDB {
    globalTouchTravel: number;
    pressDead: number;
    releaseDead: number;
}
export interface IRGBDesc {
    lightColors: number[];
    lightSwitch: number;
    reverseEffect: number;
    superResponse: number;
    lightLuminance: number;
    lightMode: number;
    lightSpeed: number;
    lightSleepDelay: number;
    color: number;
}
export interface IKeyData {
    keys: number[];
    layouts: number[];
    values: number[];
}
export interface IDKSMode {
    key: number;
    dks: number[];
    trps: number[];
    dbs: number[];
}
export interface IMPTMode {
    key: number;
    dks?: number[];
    dbs?: number[];
}
export interface IMTMode {
    key: number;
    dks: number[];
    delay: number;
}
export interface ITGLMode {
    key: number;
    dks?: number;
    delay?: number;
}
export interface IEndMode {
    key: number;
    dks?: number;
    delay?: number;
}
export interface ISOCDMode {
    key?: number;
    dks1?: number;
    mode1?: number;
    mode2?: number;
}
export interface ISOCDModeV2 {
    pos1: number;
    pos2: number;
    key1: number;
    key2: number;
    type: number;
    mode: number;
}
export interface ISOCDModeV3 {
    pos1: number;
    pos2: number;
    key1: number;
    key2: number;
    type: number;
    mode: number;
    delay: number;
}
export interface IRSMode {
    key: number;
    dks: number;
}
export interface IMacroMode {
    key: number;
    index: number;
    len: number;
    mode: number;
    num: number;
    delay: number;
}
export interface IRM6X21Mode {
    matrix6x21: number;
    datatype: number;
}
export interface IModeParam {
    key1: any;
    dks1: any;
    dks2: any;
    delay: number;
}
export interface IWriteParam {
    addr: number;
    size: number;
    codes: number[];
}
export type VersionString = `${number}.${number}.${number}`;
