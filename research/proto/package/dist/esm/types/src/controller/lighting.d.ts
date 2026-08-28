import { IKRGBDesc, ILightMode, KRGBDescs, VersionString } from '../types/interface';
export declare class LightingController {
    cmdPRGB(isrw: boolean, param?: ILightMode, version?: VersionString): Uint8Array;
    cmdSRGB(isrw: boolean, param?: ILightMode): Uint8Array;
    getPRGB(data: Uint8Array): ILightMode;
    cmdLogoRGB(isrw: boolean, param: ILightMode): Uint8Array;
    private RGB;
    private SRGB;
    cmdSingleRGB(isrw: boolean, param: IKRGBDesc): Uint8Array;
    getSingleRGB(data: Uint8Array): IKRGBDesc;
    cmdKRGB(isrw: boolean, krgbDescs: KRGBDescs): Uint8Array;
    getSpecialSingleRGB(data: Uint8Array): ILightMode;
    cmdRGBSaturation(isrw: boolean, param: number[]): Uint8Array;
}
