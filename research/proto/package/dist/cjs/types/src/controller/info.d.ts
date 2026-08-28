import { ICmd } from '../types/interface';
export declare class InfoController {
    cmd(param: ICmd): Uint8Array;
    getCmd(data: Uint8Array): string | number | boolean | {
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
    cmdSync(): Uint8Array;
    getCmdSync(data: Uint8Array): {
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
}
