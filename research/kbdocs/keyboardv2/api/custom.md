# 自定义协议接口

## 发送自定义协议数据

ServiceKeyboard.sendCustomProtocol()

**简要描述:**
发送自定义协议数据并解析设备返回结果，适用于自定义命令或 SDK 暂未封装的协议指令。


---

### 参数

| 参数名称 | 类型 | 描述 | 是否必需 | 默认值 |
|----------|------|------|----------|--------|
| `data` | `Uint8Array` | 要发送给设备的自定义协议数据。 | 是 | 无 |
| `parseResult` | `(res: Uint8Array) => T` | 解析设备返回数据的函数。不传时默认直接返回原始 `Uint8Array`。 | 否 | `(res) => res as unknown as T` |
| `timeout` | `number` | 等待设备响应的超时时间，单位为毫秒。 | 否 | `1000` |

---

### 返回值

* **总体类型:** `Promise<T>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为 `parseResult` 处理后的结果。

---

### 使用示例

```typescript
async function sendCustomCommand() {
  try {
    const data = new Uint8Array([0x01, 0x02, 0x03, 0x04]);

    const result = await ServiceKeyboard.sendCustomProtocol(data, (res) => {
      return {
        raw: res,
        success: res[0] === 0x00,
      };
    });

    console.log('自定义协议返回:', result);
  } catch (error) {
    console.error('发送自定义协议失败:', error);
  }
}

sendCustomCommand();
```

---

### 注意事项

::: tip

* 调用前请确保设备已通过 `ServiceKeyboard.init()` 初始化成功。
* `data` 需要符合设备端约定的协议格式，否则设备可能不会返回有效数据。
* 如果自定义命令处理耗时较长，可以根据实际情况增大 `timeout`。
* 建议为 `parseResult` 补充类型定义，方便业务层获得更明确的返回结构。

:::
