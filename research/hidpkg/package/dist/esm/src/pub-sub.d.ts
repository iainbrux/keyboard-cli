type EventHandler = (data: any) => void;
declare class EventEmitter {
    private events;
    private inputReportEvents;
    subscribe(eventName: string, handler: EventHandler): void;
    publish(eventName: string, data: any): void;
    unsubscribe(eventName: string, handler: EventHandler): void;
    clear(): void;
    subscribeInputReportEvent(eventName: string, handler: (data: Uint8Array) => void): void;
    publishInputReportEvent(eventName: string, data: Uint8Array): void;
    unsubscribeInputReportEvent(eventName: string): void;
}
export default EventEmitter;
