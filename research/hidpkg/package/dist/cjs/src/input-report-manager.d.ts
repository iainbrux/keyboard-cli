import EventEmitter from './pub-sub';
import { HIDDevice } from './types/types';
export declare class InputReportManager extends EventEmitter {
    private device;
    private readonly MAX_RETRIES;
    private responseQueue;
    private waitingResolvers;
    static readonly Events: {
        INPUT_REPORT: string;
        ERROR: string;
    };
    constructor(device: HIDDevice);
    reset(): void;
    private handleInputReport;
    sendReport(packet: Uint8Array): Promise<void>;
    private sendData;
    private waitForResponse;
    private loadedResponseQueue;
    private waitForResponses;
    private tryOnce;
    private attemptSend;
    sendAndWait(data: Uint8Array | Uint8Array[], options?: {
        expectedResponses?: number;
        timeout?: number;
        sendTime?: number;
    }): Promise<DataView | DataView[] | null>;
}
