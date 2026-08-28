import EventEmitter from './pub-sub';
import { Device, DeviceInit, HIDDevice } from './types/types';
type EventData = {
    deviceStatus: {
        status: string;
    };
    deviceInfo: {
        status: string;
        device: HIDDevice | null;
        deviceList: Device[];
    };
    inputReport: {
        data: DataView;
        reportId: number;
        sequence: number;
    };
    error: Error;
};
declare class WebHIDService extends EventEmitter {
    private static instance;
    static readonly Events: {
        readonly DEVICE_STATUS: "deviceStatus";
        readonly DEVICE_INFO: "deviceInfo";
        readonly INPUT_REPORT: "inputReport";
        readonly ERROR: "error";
    };
    private requestDeviceStatus;
    private hidDevices;
    private device;
    private id;
    private usage;
    private usagePage;
    private configs;
    private inputReportManager;
    private isReconnecting;
    private isInputReportListenerSetup;
    get deviceUsagePage(): number;
    private constructor();
    static getInstance({ configs, usage, usagePage }: DeviceInit): WebHIDService;
    devices(options?: {
        isUpgrading?: boolean;
    }): Promise<Device[]>;
    requestDevice(): Promise<HIDDevice | null | Error>;
    getDevices(): Promise<HIDDevice[]>;
    initAndConnectDevice(id: string): Promise<Device | null>;
    private open;
    sendData(data: Uint8Array | Uint8Array[], options?: {
        expectedResponses?: number;
        timeout?: number;
        sendTime?: number;
    }): Promise<DataView | DataView[] | null>;
    sendReportAndWaitResponse(data: Uint8Array, sendTime: number, timeout?: number): Promise<DataView | null>;
    sendMultipleReportsAndWaitResponse(dataPackets: Uint8Array[], sendTime: number, timeout?: number): Promise<DataView | null>;
    sendReportAndWaitMultipleResponses(data: Uint8Array, expectedResponses: number, sendTime: number, timeout?: number): Promise<DataView[]>;
    sendDataNoResponse(data: Uint8Array): Promise<void>;
    reconnection: (device: HIDDevice, id: string, isUpgrading?: boolean) => Promise<boolean>;
    closeDevice(): Promise<void>;
    on<T extends keyof EventData>(event: T, handler: (data: EventData[T]) => void): void;
    off<T extends keyof EventData>(event: T, handler: (data: EventData[T]) => void): void;
    private tagDevice;
    private filterHIDDevices;
    private updateDeviceStatus;
    private setupInputReportListener;
}
export default WebHIDService;
