import { IDB } from '../types/interface';
export declare class PerformanceController {
    cmdDB(isrw: boolean, param?: IDB): Uint8Array;
    rm6X21Pack(matrix6x21: number, datatype: number): Uint8Array;
    getRm6X21data(data: Uint8Array): number[][];
    getGlobalTouchTravel(data: Uint8Array): IDB;
    getSingleTravel(data: Uint8Array, decimal: number): string;
    getDksTravel(data: Uint8Array): number;
    getRtTravel(data: Uint8Array): number;
    getDpDr(data: Uint8Array): number;
    getAxis(data: Uint8Array): {
        axis: number;
    };
    getAxisList(data: Uint8Array): string | number | boolean | {
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
}
