# 基础的设备信息

模块作用：读取设备识别信息、协议版本和设备能力。调用入口为 `client.device`。

## 读取当前设备协议版本

client.device.getProtocolVersion()

**简要描述:**
读取当前设备协议版本。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<ProtocolVersion>`

**返回字段:**

| 字段              | 类型     | 说明                                  |
| ----------------- | -------- | ------------------------------------- |
| `mainVersion`     | `number` | 主版本。                              |
| `subVersion`      | `number` | 子版本。                              |
| `hardwareVersion` | `number` | 硬件版本。                            |
| `softwareVersion` | `number` | 软件版本。                            |
| `protocolVersion` | `string` | 拼接后的版本字符串，例如 `V1.0.0.1`。 |

---

### 使用示例

**调用方式:**

```ts
const version = await client.device.getProtocolVersion();
```

**调用示例:**

```ts
const version = await client.device.getProtocolVersion();
console.log(version.protocolVersion);
```

## 读取设备基础信息

client.device.getDevicesInfo()

**简要描述:**
读取设备基础信息。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<DeviceInfo>`

**返回字段:**

| 字段         | 类型     | 说明                            |
| ------------ | -------- | ------------------------------- |
| `deviceType` | `string` | 设备类型名称。                  |
| `boardId`    | `string` | 板型 ID，按十六进制字符串返回。 |
| `appVersion` | `string` | App 固件版本。                  |
| `pcbVersion` | `string` | PCB 版本。                      |
| `runMode`    | `number` | 当前运行模式。                  |
| `sn`         | `string` | 序列号。                        |
| `timestamp`  | `string` | 时间戳字符串。                  |

---

### 使用示例

**调用方式:**

```ts
const info = await client.device.getDevicesInfo();
```

**调用示例:**

```ts
const info = await client.device.getDevicesInfo();
console.log(info.boardId, info.appVersion, info.runMode);
```

## 读取设备能力信息

client.device.getDeviceAbility()

**简要描述:**
读取设备能力信息。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<DeviceAbility>`

**返回字段:**

| 字段                | 类型     | 说明                            |
| ------------------- | -------- | ------------------------------- |
| `keyboardAxisType`  | `string` | 轴体/按键类型描述。             |
| `connectMode`       | `string` | 连接模式，例如 USB、2.4G、BLE。 |
| `baseFunctions`     | `number` | 基础功能位。                    |
| `extendedFunctions` | `string` | 扩展功能描述。                  |

---

### 使用示例

**调用方式:**

```ts
const ability = await client.device.getDeviceAbility();
```

**调用示例:**

```ts
const ability = await client.device.getDeviceAbility();
console.log(ability.connectMode, ability.baseFunctions);
```
