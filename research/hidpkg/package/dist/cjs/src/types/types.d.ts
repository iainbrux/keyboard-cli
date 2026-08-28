import { LogLevel } from './enum';
export type DeviceInfo = {
    vendorId: number;
    productId?: number;
    usage: number;
    usagePage: number;
    productName?: string;
    protocol?: number;
};
export type DeviceInit = {
    configs: DeviceInfo[];
    usage: number;
    usagePage: number[];
};
export interface HIDCollectionInfo {
    usagePage: number;
    usage: number;
    inputReports: ReadonlyArray<HIDReportInfo>;
    outputReports: ReadonlyArray<HIDReportInfo>;
    featureReports: ReadonlyArray<HIDReportInfo>;
    children: ReadonlyArray<HIDCollectionInfo>;
}
export interface HIDReportInfo {
    id: number;
    items: any;
}
export interface HIDDevice extends EventTarget {
    id: string;
    collections: ReadonlyArray<HIDCollectionInfo>;
    readonly opened: boolean;
    readonly vendorId: number;
    readonly productId: number;
    readonly productName: string;
    oninputreport: (event: Event) => void | null;
    open(): Promise<void>;
    close(): Promise<void>;
    forget(): Promise<void>;
    sendReport(reportId: number, data: BufferSource): Promise<void>;
    sendFeatureReport(reportId: number, data: BufferSource): Promise<void>;
    receiveFeatureReport(reportId: number): Promise<DataView>;
}
export interface HIDInputReportEvent extends Event {
    device: HIDDevice;
    reportId: number;
    data: DataView;
}
export type Device = DeviceInfo & {
    id: string;
    productName: string;
    data?: HIDDevice;
    collections: ReadonlyArray<HIDCollectionInfo>;
};
export declare const LogColor: {
    调试: string;
    信息: string;
    警告: string;
    错误: string;
    TIMESTAMP: string;
    RESET: string;
};
export interface LogEntry {
    timestamp: number;
    level: LogLevel;
    message: string;
    data?: any;
}
export interface LoggerConfig {
    maxLogs?: number;
    logToConsole?: boolean;
    logToStorage?: boolean;
    storageKey?: string;
    colorEnabled?: boolean;
}
export interface InputReportManagerConfig {
    logger?: LoggerConfig;
    maxRetries?: number;
}
