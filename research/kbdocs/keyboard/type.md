# 类型查询

## getApi

```ts type 目前支持类型
  ORDER_TYPE_PROTOCOL_VERSION = 0x01, // 获取协议版本
  ORDER_TYPE_SAVING_PARAMETER = 0x02, // 手动保存参数
  ORDER_TYPE_RELOAD_PARAMETERS = 0x03, // 重新加载参数
  ORDER_TYPE_CLEAR_CALIBRATION_DATA = 0x04, // 清除校准数据
  ORDER_TYPE_OPEN_DKS = 0x05, // 开启 DKS
  ORDER_TYPE_CLOSE_DKS = 0x06, // 关闭 DKS
  ORDER_TYPE_REVERSE = 0x07, // 反向
  ORDER_TYPE_TURN_ON_REMAPPING = 0x08, // 开启重映射
  ORDER_TYPE_TURN_OFF_REMAPPING = 0x09, // 关闭重映射
  ORDER_TYPE_ENABLE_RELATIVE_TRIGGER = 0x0a, // 开启相对触发
  ORDER_TYPE_TURN_OFF_RELATIVE_TRIGGER = 0x0b, // 关闭相对触发
  ORDER_TYPE_START_CALIBRATION = 0x0c, // 开启校准,保留项，s_arg为霍尔电压最大最小变化范围（单位mV）
  ORDER_TYPE_CLOSE_CALIBRATION = 0x0d, // 关闭校准,保留项，s_arg =0表示已退出
  ORDER_TYPE_START_DEMONSTRATION = 0x0e, // 演示功能打开
  ORDER_TYPE_CLOSE_DEMONSTRATION = 0x0f, // 演示功能关闭
  ORDER_TYPE_ERASE_MACROSTORAGE = 0x10, // 宏参数保存前需调用该指令擦除相应的存储区
  ORDER_TYPE_RESTORE_FACTORY_SETTINGS = 0x11, // 恢复出厂设置,等待响应超时应 > 200ms
  // query 指令返回 s_arg参数为 1表示查询为真,即所查询的内容被确认为真,其它值为否
  ORDER_TYPE_LOCK_WIN_KEY = 0x20, // query, 查询锁WIN键
  ORDER_TYPE_QUERY_WIN_MODEL = 0x21, // query, 查询是否为 Win 模式
  ORDER_TYPE_QUERY_MAC_MODEL = 0x22, // query, 查询是否为 Mac 模式
  ORDER_TYPE_QUERY_STANDARD_MODEL = 0x23, // query, 查询是否为 标准 模式
  ORDER_TYPE_QUERY_ADJUSTABLE_SPEEDMODEL = 0x24, // query, 查询是否为 可调行程 模式
  ORDER_TYPE_PRECISION_STROKE = 0x25, // 8bit精度+16bit最小行程度+16bit最大行程。以um为单位，如10精度(10um)为0.01mm,50精度(50um)为0.05mm。行程范围0~4000um。
  ORDER_TYPE_KEYBOARD_NAME = 0x26, // 读取键盘名字
  ORDER_TYPE_SET_WIN_MODEL = 0x30, // switch, 切换到 Win 模式，s_arg =1表示成功
  ORDER_TYPE_SET_MAC_MODEL = 0x31, // switch, 切换到 Mac 模式，s_arg =1表示成功
  ORDER_TYPE_SET_STANDARD_MODEL = 0x32, // switch, 切换到 标准 模式
  ORDER_TYPE_SET_ADJUSTABLE_SPEEDMODEL = 0x33, // switch, 切换到 可调行程 模式
  ORDER_TYPE_TOP_DEAD_SWITCH = 0x34, // 顶部死区
  ORDER_TYPE_RGB1 = 0x40, // 写入 RGB 灰度 1, h_arg = 0xFF 为无效，s_arg表示返回灰度值
  ORDER_TYPE_RGB2 = 0x41, // 写入 RGB 灰度 2
  ORDER_TYPE_RGB3 = 0x42, // 写入 RGB 灰度 3
  ORDER_TYPE_RGB4 = 0x43, // 写入 RGB 灰度 4
  ORDER_TYPE_ROES = 0x50, // 设置回报率, h_arg / s_arg = 0[8KHz], 1[4KHz], 2[2KHz], 3[1KHz], 4[500Hz], 5[250Hz]，6[125Hz], 无效，只可用于获取当前的回报率
  ORDER_TYPE_WEB = 0x60, // 一键打开网址设置，h_arg[n] = 网址字符ascii码，最长50字节
  ORDER_TYPE_SOCD = 0x61, // 快速开启/关闭SOCD功能，h_arg=0x00关闭SOCD，h_arg=0x01开启SOCD
  ORDER_TYPE_CONFIG = 0x70, // 切换配置功能，h_arg / s_arg = 0[配置1],1[配置2],2[配置3],3[配置4]，其他参数：查询配置
  ORDER_TYPE_AXOSOME = 0x75, // 查询键盘支持的轴体ID
  ORDER_TYPE_CURRENT_AXOSOME = 0x76, // 当前键盘的轴体，s_arg = [uint16,uint]

```
