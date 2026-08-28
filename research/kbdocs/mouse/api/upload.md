# 固件升级

模块作用：固件升级链路相关命令。调用入口为 `client.upload`。

推荐链路：

```ts
await client.upload.appToBoot();
await client.upload.validateHeadData({ data: headData });
await client.upload.backupApp({ data: backupData });
await client.upload.downloadInit();

for (let index = 0; index < chunks.length; index += 1) {
  await client.upload.writeFlash({
    address: baseAddress + index * chunkSize,
    data: chunks[index],
    firstWrite: index === 0,
  });
}

await client.upload.validateAfterUpgrade();
await client.upload.bootToApp();
```

## 通知设备从 App 模式切换到 Boot 模式

client.upload.appToBoot()

**简要描述:**
通知设备从 App 模式切换到 Boot 模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<Uint8Array>`
* **描述:** 返回设备响应 payload。

---

### 使用示例

**调用方式:**

```ts
const response = await client.upload.appToBoot();
```

**调用示例:**

```ts
await client.upload.appToBoot();
```

## 通知设备从 Boot 模式切换回 App 模式

client.upload.bootToApp()

**简要描述:**
通知设备从 Boot 模式切换回 App 模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<Uint8Array>`
* **描述:** 返回设备响应 payload。

---

### 使用示例

**调用方式:**

```ts
const response = await client.upload.bootToApp();
```

**调用示例:**

```ts
await client.upload.bootToApp();
```

## 校验升级头数据

client.upload.validateHeadData(param)

**简要描述:**
校验升级头数据。

---

### 参数

| 参数         | 类型         | 是否必需 | 说明         |
| ------------ | ------------ | ---- | ------------ |
| `param.data` | `Uint8Array` | 是   | 升级头数据。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.upload.validateHeadData({ data });
```

**调用示例:**

```ts
const ok = await client.upload.validateHeadData({
  data: firmwareHead,
});
```

## 执行 App 备份命令

client.upload.backupApp(param)

**简要描述:**
执行 App 备份命令。

---

### 参数

| 参数         | 类型         | 是否必需 | 说明                 |
| ------------ | ------------ | ---- | -------------------- |
| `param.data` | `Uint8Array` | 是   | 备份命令携带的数据。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.upload.backupApp({ data });
```

**调用示例:**

```ts
await client.upload.backupApp({
  data: backupPayload,
});
```

## 初始化固件下载/写入流程

client.upload.downloadInit()

**简要描述:**
初始化固件下载/写入流程。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 当前实现只要收到响应即返回 `true`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.upload.downloadInit();
```

**调用示例:**

```ts
await client.upload.downloadInit();
```

## 写入 flash 数据块

client.upload.writeFlash(param)

**简要描述:**
写入 flash 数据块。

---

### 参数

| 参数               | 类型         | 是否必需 | 说明                                                     |
| ------------------ | ------------ | ---- | -------------------------------------------------------- |
| `param.address`    | `number`     | 是   | 写入地址，会按 32 位小端拆分。                           |
| `param.data`       | `Uint8Array` | 是   | 写入的数据块。                                           |
| `param.size`       | `number`     | 否   | 类型中保留字段，当前实现按 `data.length` 计算写入长度。  |
| `param.firstWrite` | `boolean`    | 是   | 是否为首包。首包会构造完整软件帧头，非首包返回续包数据。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.upload.writeFlash(param);
```

**调用示例:**

```ts
await client.upload.writeFlash({
  address: 0x08000000,
  data: firmwareChunk,
  firstWrite: true,
});
```

## 升级完成后校验固件

client.upload.validateAfterUpgrade()

**简要描述:**
升级完成后校验固件。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.upload.validateAfterUpgrade();
```

**调用示例:**

```ts
const ok = await client.upload.validateAfterUpgrade();
```

## 写入板型 ID 数据。该方法发送后不等待响应

client.upload.setBoardId(data)

**简要描述:**
写入板型 ID 数据。该方法发送后不等待响应。

---

### 参数

| 参数   | 类型       | 是否必需 | 说明                   |
| ------ | ---------- | ---- | ---------------------- |
| `data` | `number[]` | 是   | 板型 ID 相关字节数据。 |

---

### 返回值

* **总体类型:** `Promise<void>`

---

### 使用示例

**调用方式:**

```ts
await client.upload.setBoardId(data);
```

**调用示例:**

```ts
await client.upload.setBoardId([0x01, 0x02, 0x03, 0x04]);
```
