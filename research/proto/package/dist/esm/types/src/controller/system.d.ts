import { IWriteParam } from '../types/interface';
export declare class SystemController {
    blSIGN(unlock: number, data: any, sn: number[]): Uint8Array;
    blERASE(size: number): Uint8Array;
    blREBOOT(): Uint8Array;
    blTOAPP(size: number, crc: number): Uint8Array;
    blWRITE(param: IWriteParam): Uint8Array[];
    blRCRC(size: number): Uint8Array;
    picStart(size: number, picId: number): Uint8Array;
    picWrite(addr: number, size: number, value: number[]): Uint8Array[];
    getSignature(data: Uint8Array): {
        signSuccess: boolean;
        signature: number[];
    };
    getWrite(data: Uint8Array): {
        currentUpdateAddress: number;
    };
    getCrc(data: Uint8Array): number;
}
