import { Device } from './types/types';
type Listener = (info: {
    device: Device;
    type: 'connect' | 'disconnect';
}) => void;
declare class UsbDetect {
    private static listeners;
    private static shouldMonitor;
    private static hasMonitored;
    private static isUpgrading;
    private static isUpgradingFail;
    private static isUpgradingAfterBoot;
    private static usage;
    private static uagePage;
    private static reconnection;
    private static deviceBase;
    static get id(): string;
    private static device;
    private static onConnect;
    private static onDisconnect;
    static startMonitoring(): void;
    static stopMonitoring(): void;
    static setUpgrading(isUpgrading: boolean): void;
    static setUpgradingFail(isUpgradingFail: boolean): void;
    static setUpgradingAfterBoot(isUpgradingAfterBoot: boolean): void;
    static reset(): void;
    static subscribe(eventName: string, callback: Listener): void;
    static unsubscribe(eventName: string, callback: Listener): void;
    private static notifyListeners;
    static bindToDeviceBase(deviceBase: any): void;
}
export default UsbDetect;
