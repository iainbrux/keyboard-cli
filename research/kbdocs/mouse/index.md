# Mouse 模块方法梳理

由于浏览器安全机制问题在调用sdk时候需要走的是hid协议，所以在最开始需要进行浏览器授权，授权成功后才能调用sdk。

## 开始

### 安装依赖

```bash
pnpm add  @sparklinkplayjoy/webhid-plugin @sparklinkplayjoy/protocol-mouse

```

## 搭建项目

### 项目引用

1. 初始化 WebHID 插件

```js
import { createWebHidPlugin } from '@sparklinkplayjoy/webhid-plugin';

const plugin = createWebHidPlugin({
  filters: [
    {
      emptySummary: '未选择鼠标固件文件',
      key: 'mouse',
      label: '鼠标',
      productId: 0x1111,
      targetAddress: 0x03,
      usage: 0x01,
      usagePage: 0xffb0,
      vendorId: 0x2222,
    },
    {
      emptySummary: '未选择接收器固件文件',
      key: 'receiver',
      label: '接收器',
      productId: 0x3333,
      targetAddress: 0x05,
      usage: 0x01,
      usagePage: 0xffb0,
      vendorId: 0x6666,
    },
  ],
});
```

2. 连接设备并创建 Mouse Client

```js
import { mouseProtocolDefinition } from '@sparklinkplayjoy/protocol-mouse';

async function connectMouse() {
  // 监听 USB 设备连接/断开事件
  plugin.on('usbChange', (data) => {
    const { device } = data;

    if (data.type === 'disconnect' || data.type === 'isUpgrading_disconnect') {
    }
    if (data.type === 'connect' || (data.type === 'isUpgrading_connect' && data.reconnect)) {
      if (device?.collections?.length) {
        try {
          const targetCollection = device.collections.find(
            (collection) => collection.usage === 1 && collection.usagePage === 0xffb0,
          );

          if (targetCollection) {
          }
        } catch (error) {}
      }
    }
  });

// 🪧注意：在页面关闭时需要移除监听，否则可能会导致内存泄漏
  // plugin.off('usbChange', listener); // 移除监听

  const authorizedDevices = await plugin.getAuthorizedDevices();
  const devices = authorizedDevices.length > 0 ? authorizedDevices : await plugin.requestDevices();
  const target = devices[0];

  if (!target) {
    throw new Error('未选择设备');
  }

  const session = await plugin.open(target.id);
  // mouseProtocolDefinition 默认协议
  const client = session.createClient(mouseProtocolDefinition);

  return { client, device: target, session };
}
```

### 4. 调用方法示例

```ts
const { client } = await connectMouse();

const version = await client.device.getProtocolVersion();
console.log(version.protocolVersion);

const dpi = await client.globalSetting.getDPI();
await client.globalSetting.setDPI({
  ...dpi,
  currentDPI: 1600,
});
```