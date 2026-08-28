export declare enum DEVICE {
    INPUT_REPORT_STATUS_ACTIVE = 1,
    INPUT_REPORT_STATUS_INACTIVE = 3
}
export declare enum REQUESTDEVICESTATUS {
    CONNECT_STATUS_ACTIVE = "ACTIVE",
    CONNECT_STATUS_WAITING = "WAITING",
    CONNECT_STATUS_INACTIVE = "INACTIVE"
}
export declare enum EVENT {
    GETDEVICEINFO = "GETDEVICEINFO",
    INPUTREPORT = "INPUTREPORT",
    USBCHANGE = "usbChange",
    SWITCHCONFIG = "switchConfig",
    CUSTOMCOMMAND = "customCommand",
    LIGHTINGBASE = "lightingBase",
    TOUCHFLOW = "touchFlow",
    VOICEFLOW = "voiceFlow"
}
export declare enum LogLevel {
    DEBUG = "\u8C03\u8BD5",
    INFO = "\u4FE1\u606F",
    WARN = "\u8B66\u544A",
    ERROR = "\u9519\u8BEF"
}
