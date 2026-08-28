# 基础的设备信息

## 获取设备列表

ServiceKeyboard.getDevices()

**简要描述:**
获取连接到系统的设备列表。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<Device[]>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个 `Device` 对象数组。每个 `Device` 对象代表一个已连接的键盘设备。
* **解析对象结构 (`Device`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `data` | `object` | 设备数据对象 | - |
| `data.id` | `string` | 设备ID | `"b1545667-777b-4a8a-9640-1c000a56b5e7"` |
| `id` | `string` | 设备唯一标识符 | `"b1545667-777b-4a8a-9640-1c000a56b5e7"` |
| `usage` | `number` | 设备用途值 | `1` |
| `usagePage` | `number` | 设备用途页值 | `65440` |
| `vendorId` | `number` | 厂商ID | `7331` |
| `productId` | `number` | 产品ID | `2049` |
| `productName` | `string` | 产品名称 | `"ET65 HE"` |

---

### 使用示例

```js
async function fetchDevices() {
  try {
    const devices = await ServiceKeyboard.getDevices();
    console.log('发现的设备:', devices);
    if (devices.length > 0) {
      // 对设备列表进行操作，例如选择第一个设备进行初始化
      // const firstDevice = devices[0];
      // await ServiceKeyboard.init(firstDevice.id);
    } else {
      console.log('未检测到设备。');
    }
  } catch (error) {
    console.error('获取设备列表失败:', error);
  }
}

fetchDevices();
```

## 初始化指定的键盘设备

ServiceKeyboard.init()

**简要描述:**
初始化指定的键盘设备，以便后续进行交互。

---

### 参数

| 参数名称 | 类型     | 描述                     | 是否必需 | 默认值 |
|----------|----------|--------------------------|----------|--------|
| `id`     | `string` | 要初始化的设备的唯一标识符。可从`ServiceKeyboard.getDevices()`获取 | 是       | 无     |

---

### 返回值

* **总体类型:** `Promise<Device | null>`
* **描述:** 返回一个 `Promise`。
  * **Promise 解析 (Resolves):** `Device` - 设备初始化成功时，解析为该设备的 `Device` 对象。
  * **Promise 解析 (Resolves):** `null` - 设备初始化失败或未找到指定设备时，解析为 `null`。
* **解析对象结构 (`Device`):**
  * 关于 `Device` 对象的详细结构，请 [查看Device的类型](/keyboard/model#设备)。

---

### 使用示例

```js
async function initializeDevice(deviceId: string) {
  try {
    const device = await ServiceKeyboard.init(deviceId);
    if (device) {
      console.log('设备初始化成功:', device);
      // 现在可以使用 device 对象与设备进行交互
      // const deviceInfo = await ServiceKeyboard.getDevicesInfo(); 
    } else {
      console.log('设备初始化失败或未找到设备。');
    }
  } catch (error) {
    console.error('初始化设备时发生错误:', error);
  }
}

// deviceId 是从 getDevices() 获取到的
// const exampleDeviceId = "some_device_id_string";
// initializeDevice(exampleDeviceId);
```

## 获取设备的基础硬件和固件信息

ServiceKeyboard.getDevicesInfo()

**简要描述:**
获取当前已初始化设备的基础硬件和固件信息。

---

### 参数

此方法不需要参数 (通常在成功调用 `ServiceKeyboard.init()` 后使用)。

---

### 返回值

* **总体类型:** `Promise<BaseInfo>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含设备基础信息的 `BaseInfo` 对象。
* **解析对象结构 (`BaseInfo`):**

| 字段名称          | 类型     | 描述                     | 示例值  |
|-------------------|----------|--------------------------|---------------|
| `type`            | `number` | 设备类型。               | `1`           |
| `subType`         | `number` | 设备子类型。             | `0`           |
| `boardId`         | `number` | 板卡ID。                 | `3145729`     |
| `appVersion`      | `string` | 固件的应用版本号。       | `'1.0.1.0'`   |
| `pcbVersion`      | `string` | PCB版本号。              | `'1-0-0-0'`   |
| `runModeVersion`  | `number` | 运行模式版本。           | `0`           |
| `sn`              | `string` | 键盘的序列号。           | `'54817806657765148212547'` |
| `timestamp`       | `string` | 时间戳。                 | `'2025052215:4'` |

---

### 使用示例

```js
async function fetchBaseDeviceInfo() {
  try {
    // 确保设备已初始化
    // await ServiceKeyboard.init(deviceId);
    const baseInfo = await ServiceKeyboard.getDevicesInfo();
    console.log('设备基础信息:', baseInfo);
    // console.log('固件版本:', baseInfo.appVersion);
  } catch (error) {
    console.error('获取设备基础信息失败:', error);
  }
}

// fetchBaseDeviceInfo(); // 需在设备初始化后调用
```

## 获取协议版本

ServiceKeyboard.getProtocolVersion()

**简要描述:**
获取键盘设备的协议版本信息。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<ProtocolVersion>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含协议版本信息的对象。
* **解析对象结构 (`ProtocolVersion`):**

| 字段名称          | 类型     | 描述                     | 示例值  |
|-------------------|----------|--------------------------|---------|
| `mainVersion`     | `number` | 主版本号。               | `1`     |
| `subVersion`      | `number` | 子版本号。               | `0`     |
| `hardwareVersion` | `number` | 硬件版本号。             | `1`     |
| `softwareVersion` | `number` | 软件版本号。             | `0`     |

---

### 使用示例

```js
async function fetchProtocolVersion() {
  try {
    const versionInfo = await ServiceKeyboard.getProtocolVersion();
    console.log('协议版本信息:', versionInfo);
  } catch (error) {
    console.error('获取协议版本信息失败:', error);
  }
}

// fetchProtocolVersion();
```

## 获取配置列表

ServiceKeyboard.getConfigList()

**简要描述:**
获取键盘可用的配置列表信息。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<ConfigList>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含配置列表信息的对象。
* **解析对象结构 (`ConfigList`):**

| 字段名称 | 类型     | 描述                     | 示例值  |
|----------|----------|--------------------------|---------|
| `total`  | `number` | 配置总数。               | `4`     |
| `list`   | `string[]` | 配置名称列表。         | `['Config1', 'Config2', 'Config3', 'Config4']` |

---

### 使用示例

```js
async function fetchConfigList() {
  try {
    const configList = await ServiceKeyboard.getConfigList();
    console.log('配置列表:', configList);
    console.log('配置总数:', configList.total);
    console.log('配置名称列表:', configList.list);
  } catch (error) {
    console.error('获取配置列表失败:', error);
  }
}

// fetchConfigList();
```

## 获取当前配置

ServiceKeyboard.getConfig()

**简要描述:**
获取键盘当前使用的配置信息。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<Config>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含当前配置信息的对象。
* **解析对象结构 (`Config`):**

| 字段名称 | 类型     | 描述                     | 示例值  |
|----------|----------|--------------------------|---------|
| `key`    | `string` | 当前配置的名称。         | `'Config2'` |
| `value`  | `number` | 当前配置的索引值。       | `1`     |

---

### 使用示例

```js
async function fetchCurrentConfig() {
  try {
    const config = await ServiceKeyboard.getConfig();
    console.log('当前配置名称:', config.key);
    console.log('当前配置索引:', config.value);
  } catch (error) {
    console.error('获取当前配置失败:', error);
  }
}

// fetchCurrentConfig();
```

## 设置当前配置

ServiceKeyboard.setConfig()

**简要描述:**
设置键盘当前使用的配置。

---

### 参数

| 参数名称 | 类型     | 描述                     | 是否必需 | 默认值 |
|----------|----------|--------------------------|----------|--------|
| `config` | `string` | 要设置的配置名称。       | 是       | 无     |

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 返回一个 `Promise`。操作成功完成时，`Promise` 解析，无特定返回值。如果设置失败，`Promise` 会拒绝并返回一个错误。

---

### 使用示例

```js
async function setCurrentConfig(configName) {
  try {
    await ServiceKeyboard.setConfig(configName);
    console.log(`已切换到配置: ${configName}`);
  } catch (error) {
    console.error('设置配置失败:', error);
  }
}

// 示例：切换到 Config1
// setCurrentConfig('Config1');
```

### 注意事项

::: tip

* `index`: 配置索引值必须在1到4之间。
* 切换配置后，建议监听 `getCmd` 事件来获取配置切换的结果。
* 在收到配置切换成功的通知后，通常需要重新获取以下数据：
  * 键盘配置信息
  * 灯光设置
  * 按键映射
  * 其他相关设置
* 建议在配置切换过程中显示加载状态，以提供更好的用户体验。
:::

## 获取配置名称

ServiceKeyboard.getConfigName(index)

**简要描述:**
根据配置索引获取配置名称。

---

### 参数

| 参数名称 | 类型     | 描述               | 是否必需 | 默认值 |
|----------|----------|--------------------|----------|--------|
| `index`  | `number` | 配置索引（1-4）。  | 是       | 无     |

---

### 返回值

* **总体类型:** `Promise<string>`
* **描述:** 返回该索引对应的配置名称。

---

### 使用示例

```js
async function fetchConfigName(index) {
  // 需协议版本 >= v1.0.3.0（可通过 ServiceKeyboard.getProtocolVersion 校验）
  const name = await ServiceKeyboard.getConfigName(index);
  console.log('配置名称:', name);
  return name;
}
```

---

### 注意事项

:::: tip

* 需要设备协议版本至少为 `v1.0.3.0`。
* 协议版本可通过 `ServiceKeyboard.getProtocolVersion()` 获取。
* `index` 建议传入有效范围（通常 1-4）。
::::

## 设置配置名称

ServiceKeyboard.setConfigName(index, name)

**简要描述:**
设置指定配置索引的配置名称。

---

### 参数

| 参数名称 | 类型     | 描述               | 是否必需 | 默认值 |
|----------|----------|--------------------|----------|--------|
| `index`  | `number` | 配置索引（1-4）。  | 是       | 无     |
| `name`   | `string` | 要设置的配置名称。 | 是       | 无     |

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 设置成功解析为 `void`，失败时抛出错误。

---

### 使用示例

```js
async function updateConfigName(index, name) {
  // 需协议版本 >= v1.0.3.0（可通过 ServiceKeyboard.getProtocolVersion 校验）
  await ServiceKeyboard.setConfigName(index, name);
  console.log(`已设置配置 ${index} 的名称为: ${name}`);
}
```

---

### 注意事项

:::: tip

* 需要设备协议版本至少为 `v1.0.3.0`。
* 建议对 `name` 做长度与字符集校验，避免设备端写入失败。
* 设置后可调用 `ServiceKeyboard.getConfigName(index)` 读取校验。
::::

## 回报率列表查询

ServiceKeyboard.getRateOfReturnList()

**简要描述:**
获取设备支持的回报率列表。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<{ list: string[] }>`
* **描述:** 返回包含回报率字符串列表，如 `['R8','R16', ...]`。

---

### 使用示例

```js
async function fetchRateOfReturnList() {
  // 需协议版本 >= v1.0.7.0
  const { list } = await ServiceKeyboard.getRateOfReturnList();
  // 若需要下拉使用，可转成 {label,value,key}
  return list.map((rate, index) => ({ label: rate.replace('R', ''), value: index, key: rate }));
}
```

---

### 注意事项

:::: tip

* 需要设备协议版本至少为 `v1.0.7.0`。
* 列表项通常以 `R` 加速率数字的形式返回，例如 `R8KHz`。
::::

## 获取当前回报率

ServiceKeyboard.getRateOfReturn()

**简要描述:**
获取设备当前使用的回报率。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<{ key: string }>`
* **描述:** 返回当前回报率的值，格式为 "R8KHz" 这样的字符串。

**返回值示例:**

```js
{
  "key": "R8KHz"
}
```

---

### 使用示例

```js
async function fetchCurrentRateOfReturn() {
  // 需协议版本 >= v1.0.7.0
  const { key } = await ServiceKeyboard.getRateOfReturn();
  return key;
}
```

---

### 注意事项

:::: tip

* 需要设备协议版本至少为 `v1.0.7.0`。
* 返回值格式为 "R8KHz" 这样的字符串，表示当前使用的回报率。
::::

## 设置回报率

ServiceKeyboard.setRateOfReturn(value)

**简要描述:**
设置 USB 2.0 设备的回报率。此接口仅适用于 USB 2.0 设备，无需配合 `setUSBMode` 使用。

---

### 参数

| 参数名称 | 类型     | 描述                                 | 是否必需 | 默认值 |
|----------|----------|--------------------------------------|----------|--------|
| `value`  | `string` | 目标回报率值，格式为 "R8KHz" 这样的字符串。 | 是       | 无     |

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 设置成功解析为 `void`，失败时抛出错误。

---

### 使用示例

```js
async function updateRateOfReturn(value) {
  // 仅适用于 USB 2.0 设备
  await ServiceKeyboard.setRateOfReturn(value);
}

// 示例：设置为 8KHz 回报率
updateRateOfReturn('R8KHz');

// 示例：设置为 1KHz 回报率
updateRateOfReturn('R1KHz');
```

**完整回报率设置流程示例:**

对于 USB 2.0 设备，只需要调用 `setRateOfReturn` 即可设置回报率：

```js
async function setUSBDongleReportRate(rateValue) {
  // USB 2.0 设备：只需调用 setRateOfReturn
  await ServiceKeyboard.setRateOfReturn(rateValue);
  console.log('已设置为回报率:', rateValue);
}

// 示例：设置为 8K 回报率
setUSBDongleReportRate('R8KHz');

// 示例：设置为 1K 回报率
setUSBDongleReportRate('R1KHz');
```

---

### 注意事项

::: tip

* **重要**: 此接口仅适用于 USB 2.0 设备。
* USB 3.0 设备请使用 `setUSBMode` 接口设置回报率。
* 需要设备协议版本至少为 `v1.0.7.0`。
* 参数格式为 "R8KHz" 这样的字符串，与 `getRateOfReturnList()` 返回的列表项格式一致。
* 建议在设置后再次调用 `getRateOfReturn()` 校验结果，或根据需要刷新 UI。

:::

## 获取 RT 精度

ServiceKeyboard.getRtPrecision()

**简要描述:**
获取设备支持的RT最小精度（步进最小值）。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<{ min: number }>`
* **描述:** 返回最小步进精度，单位与设备定义一致（例如毫秒）。

**返回值示例:**

```js
{
  "min": 0.001
}
```

---

### 使用示例

```js
async function fetchRtPrecision() {
  const { min } = await ServiceKeyboard.getRtPrecision();
  console.log('RT精度最小步进:', min);
  return min;
}
```

## 获取设备功能规格

ServiceKeyboard.getDeviceFunction()

**简要描述:**
获取设备的功能规格信息，包括连接模式等。协议版本 1.1.6.0 及以上可用。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<IDeviceFunction>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为包含设备功能规格信息的对象。
* **解析对象结构 (`IDeviceFunction`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `connectionMode` | `object` | 连接模式信息 | - |
| `connectionMode.usb3` | `boolean` | 是否为 USB 3.0 设备，true 表示支持 USB 3.0 | `true` |

**返回值示例:**

```js
{
  "connectionMode": {
    "usb3": true
  }
}
```

---

### 使用示例

```js
async function fetchDeviceFunction() {
  // 需协议版本 >= v1.1.6.0
  const functionInfo = await ServiceKeyboard.getDeviceFeature();
  console.log('设备功能规格:', functionInfo);
  
  // 判断是否为 USB 3.0 设备
  const isUSBDevice = functionInfo?.connectionMode && functionInfo?.connectionMode.usb3;
  console.log('是否为 USB 3.0 设备:', isUSBDevice);
  
  return functionInfo;
}
```

---

### 注意事项

::: tip

* 需要设备协议版本至少为 `v1.1.6.0`。
* 当 `connectionMode.usb3` 为 `true` 时，表示该设备为 USB 3.0 设备，可以开启 USB 3.0 高回报率模式。
* USB 3.0 设备相比 USB 2.0 设备支持更高的回报率，可实现更低的输入延迟。
* 建议根据此接口的返回值判断是否显示 USB 3.0 模式选项。

:::

## 获取USB模式状态

ServiceKeyboard.getUSBModeStatus()

**简要描述:**
获取 USB 模式状态信息，包括当前模式和驱动状态。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<{ mode: number; driverStatus: number; vid: number; pid: number; bootVid: number; bootPid: number }>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为包含 USB 模式状态信息的对象。
* **解析对象结构:**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `mode` | `number` | USB 运行态模式 | `1` |
| `driverStatus` | `number` | 驱动状态，详见下表 | `3` |
| `vid` | `number` | 当前 USB 厂商 ID（十六进制），用于获取轴体和固件接口 | `7331` |
| `pid` | `number` | 当前 USB 产品 ID（十六进制），用于获取轴体和固件接口 | `2049` |
| `bootVid` | `number` | Boot 模式 USB 厂商 ID（十六进制） | `7331` |
| `bootPid` | `number` | Boot 模式 USB 产品 ID（十六进制） | `2048` |

**返回值示例:**

```js
{
  "mode": 1,
  "driverStatus": 3,
  "vid": 7331,
  "pid": 2049,
  "bootVid": 7331,
  "bootPid": 2048
}
```

**mode 值说明:**

| 值 | 描述 |
|-----|------|
| 0 | USB 2.0 模式 |
| 1 | USB 3.0 模式 |

**driverStatus 值说明:**

| 值 | 描述 |
|-----|------|
| 0 | 未安装未枚举成功 USB 3.0 驱动 |
| 1 | 枚举USB成功 |
| 2 | USB 3.0 驱动枚举成功（已安装驱动） |
| 3 | USB 3.0 驱动枚举成功（已安装驱动），建议用此值判断设备是否安装驱动成功 |

---

### 使用示例

```js
async function fetchUSBModeStatus() {
  const res = await ServiceKeyboard.getUSBModeStatus();
  console.log('USB模式状态:', res);
  console.log('当前模式:', res.mode);
  // 判断驱动安装状态
  if (res.driverStatus === 3) {
    console.log('驱动状态: 已安装USB 3.0驱动');
  } else if (res.driverStatus === 2) {
    console.log('驱动状态: USB 3.0 驱动枚举成功');
  } else if (res.driverStatus === 1) {
    console.log('驱动状态: 枚举USB成功');
  } else {
    console.log('驱动状态: 未安装未枚举成功 USB 3.0 驱动');
  }
  // 使用 vid 和 pid 用于获取轴体和固件接口
  console.log('当前 VID:', res.vid);
  console.log('当前 PID:', res.pid);
  console.log('Boot VID:', res.bootVid);
  console.log('Boot PID:', res.bootPid);
  return res;
}
```

---

### 注意事项

::: tip

* 此接口用于获取当前的 USB 模式状态和驱动安装状态。
* `driverStatus` 建议使用值 `3` 判断设备是否安装驱动成功。
* 可结合 `getDeviceFunction()` 接口判断设备是否支持 USB 3.0。
* USB 3.0 模式需要安装对应的驱动才能正常工作。
* `vid` 和 `pid` 用于获取轴体列表和固件升级接口，这些值在切换 USB 模式后可能会变化。
* `bootVid` 和 `bootPid` 用于 Boot 模式下的固件升级接口。

:::

## 获取USB模式存储值

ServiceKeyboard.getUSBModeStored()

**简要描述:**
获取 USB 模式存储值，即用户设置的 USB 模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<{ mode: number; pollingRate: string }>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为包含 USB 模式存储值的对象。
* **解析对象结构:**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `mode` | `number` | USB 模式存储值 | `2` |
| `pollingRate` | `string` | 当前回报率 | `"R8KHz"` |

**返回值示例:**

```js
{
  "mode": 2,
  "pollingRate": "R8KHz"
}
```

**pollingRate 值说明:**

| 值 | 描述 |
|-----|------|
| `R8KHz` | 8K 回报率 |
| `R4KHz` | 4K 回报率 |
| `R2KHz` | 2K 回报率 |
| `R1KHz` | 1K 回报率 |
| `R500Hz` | 500Hz 回报率 |
| `R250Hz` | 250Hz 回报率 |
| `R125Hz` | 125Hz 回报率 |

**mode 值说明:**

| 值 | 描述 |
|-----|------|
| 0 | USB 2.0 |
| 1 | USB 3.0 8K |
| 2 | USB 3.0 16K |
| 3 | USB 3.0 32K |

---

### 使用示例

```js
async function fetchUSBModeStored() {
  const res = await ServiceKeyboard.getUSBModeStored();
  console.log('USB模式存储值:', res.mode);
  console.log('当前回报率:', res.pollingRate);
  return res;
}
```

---

## 设置USB模式

ServiceKeyboard.setUSBMode(mode, pollingRate?)

**简要描述:**
设置 USB 模式，用于切换不同的 USB 回报率模式。

---

### 参数

| 参数名称 | 类型 | 必填 | 描述 |
|---------|------|------|------|
| `mode` | `string` | 是 | USB 模式，支持的值：`USB2_0`, `USB3_0_8K`, `USB3_0_16K`, `USB3_0_32K` |
| `pollingRate` | `string` | 否 | 回报率，支持的值为：`R8KHz`, `R4KHz`, `R2KHz`, `R1KHz`, `R500Hz`, `R250Hz`, `R125Hz` |

**mode 值说明:**

| 值 | 描述 |
|-----|------|
| `USB2_0` | USB 2.0 模式 |
| `USB3_0_8K` | USB 3.0 8K 回报率模式 |
| `USB3_0_16K` | USB 3.0 16K 回报率模式 |
| `USB3_0_32K` | USB 3.0 32K 回报率模式 |

**pollingRate 值说明:**

| 值 | 描述 |
|-----|------|
| `R8KHz` | 8K 回报率 |
| `R4KHz` | 4K 回报率 |
| `R2KHz` | 2K 回报率 |
| `R1KHz` | 1K 回报率 |
| `R500Hz` | 500Hz 回报率 |
| `R250Hz` | 250Hz 回报率 |
| `R125Hz` | 125Hz 回报率 |

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 设置成功解析为 `void`，失败时抛出错误。

---

### 使用示例

```js
async function updateUSBMode(mode, pollingRate) {
  // 设置 USB 模式
  await ServiceKeyboard.setUSBMode(mode, pollingRate);
  console.log('已设置USB模式:', mode);
}

// 示例：USB 3.0 设备设置为 16K 回报率（无需调用 setRateOfReturn）
updateUSBMode('USB3_0_16K');

// 示例：USB 3.0 设备设置为 32K 回报率（无需调用 setRateOfReturn）
updateUSBMode('USB3_0_32K');

// 示例：USB 2.0 设备设置为 1K 回报率（需要传入 pollingRate 参数，无需调用 setRateOfReturn）
updateUSBMode('USB2_0', 'R1KHz');

// 示例：USB 2.0 设备设置为 8K 回报率（需要传入 pollingRate 参数，无需调用 setRateOfReturn）
updateUSBMode('USB2_0', 'R8KHz');
```

**USB 3.0 设备回报率设置示例:**

对于 USB 3.0 设备，只需要调用 `setUSBMode` 设置对应的 USB 3.0 模式即可，无需调用 `setRateOfReturn`：

```js
async function setUSB3ReportRate16K() {
  // USB 3.0 设备：设置为 16K 回报率
  await ServiceKeyboard.setUSBMode('USB3_0_16K');
  console.log('已设置为 USB 3.0 16K 回报率');
}

async function setUSB3ReportRate32K() {
  // USB 3.0 设备：设置为 32K 回报率
  await ServiceKeyboard.setUSBMode('USB3_0_32K');
  console.log('已设置为 USB 3.0 32K 回报率');
}
```

**USB 2.0 设备回报率设置示例:**

对于 USB 2.0 设备，需要调用 `setUSBMode` 并传入 `pollingRate` 参数，无需调用 `setRateOfReturn`：

```js
async function setUSB2ReportRate(rateValue) {
  // USB 2.0 设备：调用 setUSBMode 并传入 pollingRate
  await ServiceKeyboard.setUSBMode('USB2_0', rateValue);
  console.log('已设置为 USB 2.0 回报率:', rateValue);
}

// 示例：设置为 8K 回报率
setUSB2ReportRate('R8KHz');

// 示例：设置为 1K 回报率
setUSB2ReportRate('R1KHz');
```

---

### 注意事项

::: tip

* **USB 3.0 设备**: 只需要调用 `setUSBMode` 设置对应的 USB 3.0 模式（如 `USB3_0_16K`、`USB3_0_32K`）即可，无需调用 `setRateOfReturn`。
* **USB 2.0 设备**: 只需要调用 `setUSBMode('USB2_0', pollingRate)` 并传入 `pollingRate` 参数，无需调用 `setRateOfReturn`。
* USB 3.0 高回报率模式需要设备支持 USB 3.0，可通过 `getUSBModeStatus()` 接口查询驱动状态。
* 切换 USB 模式后，建议调用 `getUSBModeStored()` 验证设置是否成功。
* USB 3.0 高回报率模式（16K/32K）可实现更低的输入延迟，适合游戏等对响应速度要求较高的场景。

:::

## 高回报率重置

ServiceKeyboard.highPollingRateReset()

**简要描述:**
在 USB 3.0 高回报率模式下，用于恢复出厂设置时重置设备。此接口会重置 USB 模式到默认状态。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 操作成功解析为 `void`，失败时抛出错误。

---

### 使用示例

```js
async function resetHighPollingRate() {
  try {
    await ServiceKeyboard.highPollingRateReset();
    console.log('高回报率重置成功');
  } catch (error) {
    console.error('高回报率重置失败:', error);
  }
}

// 在 USB 3.0 高回报率模式下恢复出厂设置时调用
resetHighPollingRate();
```

---

### 注意事项

::: tip

* 此接口主要用于 USB 3.0 高回报率模式下恢复出厂设置时的设备重置。
* 调用此接口后，USB 模式会被重置为默认状态。
* 建议在恢复出厂设置流程中调用此接口，以确保设备状态正确。

:::

## 获取系统睡眠时间（分钟）

ServiceKeyboard.getLightingSleepTime()

**简要描述:**
获取键盘系统睡眠时间（单位：分钟）。0 表示永不休眠，1 表示 1 分钟，2 表示 2 分钟，依次类推。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<number>`
* **描述:** 返回系统睡眠时间（分钟）。

**返回值示例:**

```js
3 // 表示 3 分钟
```

---

### 使用示例

```js
async function fetchLightingSleepTime() {
  // 需协议版本 >= v1.0.4.0
  const minutes = await ServiceKeyboard.getLightingSleepTime();
  console.log('系统睡眠时间(分钟):', minutes);
  return minutes;
}
```

---

### 注意事项

:::: tip

* 需要设备协议版本至少为 `v1.0.4.0`。
* 0 表示永不休眠；正整数表示分钟数。
::::

## 设置系统睡眠时间（分钟）

ServiceKeyboard.setLightingSleepTime(time)

**简要描述:**
设置键盘系统睡眠时间（单位：分钟）。0 表示永不休眠，1 表示 1 分钟，2 表示 2 分钟，依次类推。

---

### 参数

| 参数名称 | 类型     | 描述                 | 是否必需 | 默认值 |
|----------|----------|----------------------|----------|--------|
| `time`   | `number` | 睡眠时间（分钟）。   | 是       | 无     |

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 设置成功解析为 `void`，失败时抛出错误。

---

### 使用示例

```js
async function updateLightingSleepTime(time) {
  // 需协议版本 >= v1.0.4.0
  await ServiceKeyboard.setLightingSleepTime(time);
  console.log('已设置系统睡眠时间(分钟):', time);
}
```

---

### 注意事项

:::: tip

* 需要设备协议版本至少为 `v1.0.4.0`。
* 0 表示永不休眠；建议根据需求限制为合理的分钟数范围。
::::

## 关闭当前设备

ServiceKeyboard.closeDevice()

**简要描述:**
关闭当前已初始化并连接的设备，会释放与该设备的会话。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 操作成功解析为 `void`；如关闭失败会抛出错误。

---

### 使用示例

```js
async function closeCurrentDevice() {
  try {
    await ServiceKeyboard.closeDevice();
    console.log('当前设备已关闭');
  } catch (error) {
    console.error('关闭设备失败:', error);
  }
}

// closeCurrentDevice();
```

## 重新连接设备

ServiceKeyboard.reconnection()

**简要描述:**
尝试重新连接到指定的设备。

---

### 参数

| 参数名称   | 类型     | 描述                           | 是否必需 | 默认值 |
|------------|----------|--------------------------------|----------|--------|
| `device`   | `Device` | 要重新连接的 `Device` 对象实例。 | 是       | 无     |
| `deviceId` | `string` | 要重新连接的设备的唯一标识符。   | 是       | 无     |

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 返回一个 `Promise`。操作成功完成时，`Promise` 解析，无特定返回值。如果重连失败，`Promise` 会拒绝并返回一个错误。

---

### 使用示例

```js

const listener = async ({ device, type }) => {
  if (type === 'disconnect') {
    // 设备断开连接
    if (device?.collections?.length) {
      try {
        const targetCollection = device.collections.find(
          (collection) => collection.usage === 1 && collection.usagePage === 0xffb0,
        );
        if (targetCollection) {
          // 处理设备断开连接后的逻辑
        }
      } catch (error) {
        console.error('设备断开连接处理失败:', error);
      }
    }
  } 
  
  if (type === 'connect') {
    if (device?.collections?.length) {
      try {
        const targetCollection = device.collections.find(
          (collection) => collection.usage === 1 && collection.usagePage === 0xffb0,
        );
        if (targetCollection) {
          // 处理设备连接后的逻辑
        }
      } catch (error) {
        console.error('设备连接处理失败:', error);
      }
    }
  }
}

ServiceKeyboard.on('usbChange', listener);

// 移除监听
ServiceKeyboard.off('usbChange', listener);

// 在插拔事件、恢复出厂、升级结束后需要重连
```

## 设备插拔事件

ServiceKeyboard.on('usbChange', callback)

**简要描述:**
监听设备的插拔事件，包括普通连接/断开、升级过程中的连接/断开等状态变化。

---

### 参数

| 参数名称   | 类型     | 描述                           | 是否必需 | 默认值 |
|------------|----------|--------------------------------|----------|--------|
| `eventName`| `string` | 事件名称，固定为 'usbChange'    | 是       | 无     |
| `callback` | `function` | 事件回调函数，接收设备状态变化信息 | 是     | 无     |

---

### 回调函数参数

| 参数名称 | 类型     | 描述                           |
|----------|----------|--------------------------------|
| `data`   | `object` | 包含设备状态变化信息的对象      |
| `data.type` | `string` | 事件类型，可能的值包括：<br>- 'connect': 设备连接<br>- 'disconnect': 设备断开<br>- 'isUpgrading_connect': 升级过程中设备连接<br>- 'isUpgrading_disconnect': 升级过程中设备断开 |
| `data.device` | `Device` | 设备对象，包含设备信息 |
| `data.updateFail` | `boolean` | 是否是在更新写入数据中更新失败了，比如在更新中过程中拔掉了键盘 |
| `data.reconnect` | `boolean` | 是否已经重新连接成功 |

---

### 使用示例

```js
ServiceKeyboard.on('usbChange', (data) => {
  console.log('USB设备变化:', data);
  const { device } = data;
  
  if (data.updateFail) {
    // 处理更新失败的情况
  }
  
  if (data.type === 'disconnect' || data.type === 'isUpgrading_disconnect') {
    // 设备断开连接
    if (device?.collections?.length) {
      try {
        const targetCollection = device.collections.find(
          (collection) => collection.usage === 1 && collection.usagePage === 0xffb0,
        );
        if (targetCollection) {
          // 处理设备断开连接后的逻辑
        }
      } catch (error) {
        console.error('设备断开连接处理失败:', error);
      }
    }
  }

  if (data.type === 'connect' || (data.type === 'isUpgrading_connect' && data.reconnect)) {
    if (device?.collections?.length) {
      try {
        const targetCollection = device.collections.find(
          (collection) => collection.usage === 1 && collection.usagePage === 0xffb0,
        );
        if (targetCollection) {
          // 处理设备连接后的逻辑
        }
      } catch (error) {
        console.error('设备连接处理失败:', error);
      }
    }
  }
});
```

---

### 注意事项

::: tip

* 在恢复出厂设置、固件升级等操作后，会多次触发设备的插拔事件。
* 建议在设备连接后重新获取设备信息、配置信息等数据。
* 在升级过程中的设备断开和重连需要特殊处理，确保升级流程的完整性。
* 可以通过 `data.updateFail` 判断是否在写入数据中更新失败，进行相应的错误处理。
* 在插拔事件里已经处理好了设备的自动重连，不需要再调用重连接口了
:::

## 键盘配置切换事件

ServiceKeyboard.on('switchConfig', callback)

**简要描述:**
监听键盘配置切换事件。当用户通过快捷键切换键盘配置时，会触发此事件并接收固件下发的数据。

---

### 参数

| 参数名称   | 类型     | 描述                           | 是否必需 | 默认值 |
|------------|----------|--------------------------------|----------|--------|
| `eventName`| `string` | 事件名称，固定为 'switchConfig'    | 是       | 无     |
| `callback` | `function` | 事件回调函数，接收配置切换信息 | 是     | 无     |

---

### 回调函数参数

| 参数名称 | 类型     | 描述                           |
|----------|----------|--------------------------------|
| `data`   | `any` | 固件下发的配置切换数据      |

---

### 使用示例

```js
ServiceKeyboard.on('switchConfig', async (data) => {
  console.log('键盘配置切换:', data);
  // 处理配置切换后的逻辑，例如重新获取配置信息
});
```

---

### 注意事项

::: tip

* 当用户通过键盘快捷键切换配置时，会触发此事件。
* 建议在收到配置切换事件后，重新获取相关的配置信息、灯光设置、按键映射等数据。
* 可以通过此事件同步更新UI显示，确保界面与设备状态一致。
:::

## 灯光监听事件

ServiceKeyboard.on('lightingBase', callback)

**简要描述:**
监听灯光相关事件。当固件下发灯光数据时，会触发此事件。

---

### 参数

| 参数名称   | 类型     | 描述                           | 是否必需 | 默认值 |
|------------|----------|--------------------------------|----------|--------|
| `eventName`| `string` | 事件名称，固定为 'lightingBase'    | 是       | 无     |
| `callback` | `function` | 事件回调函数，接收灯光数据信息 | 是     | 无     |

---

### 回调函数参数

| 参数名称 | 类型     | 描述                           |
|----------|----------|--------------------------------|
| `data`   | `any` | 固件下发的灯光数据      |

---

### 使用示例

```js
ServiceKeyboard.on('lightingBase', async (data) => {
  console.log('灯光数据:', data);
  // 处理灯光数据，例如更新灯光设置UI
});
```

---

### 注意事项

::: tip

* 当固件下发灯光相关数据时，会触发此事件。
* 可以通过此事件实时同步灯光状态，确保界面与设备灯光状态一致。
* 建议在收到灯光数据后，更新相应的UI显示。
:::

## 进入Bootloader模式

ServiceKeyboard.appToBoot()

**简要描述:**
使设备进入 Bootloader 模式，通常用于擦除或准备更新固件。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 返回一个 `Promise`。操作成功完成时，`Promise` 解析，无特定返回值。如果操作失败，`Promise` 会拒绝并返回一个错误。

---

### 使用示例

```js
async function enterBootloaderMode() {
  try {
    // 确保设备已初始化
    // await ServiceKeyboard.init(deviceId);
    console.log('正在尝试使设备进入 Bootloader 模式...');
    await ServiceKeyboard.appToBoot();
    console.log('设备已成功进入 Bootloader 模式或相关指令已发送。');
    // 此时设备可能已断开连接，需要重新扫描或等待设备以 Bootloader 模式出现
  } catch (error) {
    console.error('使设备进入 Bootloader 模式失败:', error);
  }
}

// enterBootloaderMode();
```

---

### 注意事项

::: tip

* 执行此操作后，设备通常会断开连接并以 Bootloader 模式重新枚举（如果支持）。您可能需要重新扫描设备或使用特定的 Bootloader 工具进行后续操作（如固件更新）。
* tip: 擦除后，需要重新连接设备,调用重连接口`ServiceKeyboard.reconnection`。
:::

## 从Bootloader模式返回应用模式

ServiceKeyboard.bootToApp()

**简要描述:**
使设备从 Bootloader 模式返回到应用模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 返回一个 `Promise`。操作成功完成时，`Promise` 解析，无特定返回值。如果操作失败，`Promise` 会拒绝并返回一个错误。

---

### 使用示例

```js
async function returnToAppMode() {
  try {
    console.log('正在尝试使设备从 Bootloader 模式返回到应用模式...');
    await ServiceKeyboard.bootToApp();
    console.log('设备已成功返回到应用模式。');
    // 此时设备可能已断开连接，需要重新扫描或等待设备以应用模式出现
  } catch (error) {
    console.error('使设备返回到应用模式失败:', error);
  }
}

// returnToAppMode();
```

---

### 注意事项

::: tip

* 执行此操作后，设备通常会断开连接并以应用模式重新枚举。您需要重新扫描设备或等待设备重新连接。
* 建议在操作完成后重新连接设备，调用重连接口 `ServiceKeyboard.reconnection`。
:::

## Bin文件更新固件

ServiceKeyboard.upgrade()

**简要描述:**
向处于 Bootloader 模式的设备更新固件。

---

### 参数

| 参数名称   | 类型                                                  | 描述                                                           | 是否必需 | 默认值 |
|------------|-------------------------------------------------------|----------------------------------------------------------------|----------|--------|
| `buffer`   | `ArrayBuffer`                                         | 包含新固件二进制数据的 `ArrayBuffer`。                          | 是       | 无     |
| `callback` | `(data: {current: number, total: number, updateStatus: string, percentage: number}) => void` | 一个回调函数，用于报告固件更新的进度。`data.current` 是已传输的字节数，`data.total` 是总字节数，`data.updateStatus` 是当前更新状态，`data.percentage` 为当前进度百分比（0-100）。 | 否       | 无     |
| `options`  | `object`                                              | 更新过程中的配置选项。(可根据实际设备情况去调试)                                          | 否       | 有     |
| `options.toBootDelay` | `number`                                        | 进入 Bootloader 模式后的延迟时间（毫秒）。                      | 否       | 4000   |
| `options.toAppDelay`  | `number`                                        | 返回应用模式时的延迟时间（毫秒）。                              | 否       | 4000   |

---

### 返回值

* **总体类型:** `Promise<{ success: boolean }>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，指明固件更新是否成功。
* **解析对象结构:**

| 字段名称  | 类型      | 描述             | 示例值  |
|-----------|-----------|------------------|---------|
| `success` | `boolean` | 指示更新是否成功。 | `true`  |

---

### 使用示例

```js
async function performFirmwareUpdate(firmwareBuffer: ArrayBuffer) {
  try {
    // 确保设备处于 Bootloader 模式
    // await ServiceKeyboard.toBoot();
    // 等待设备以 Bootloader 模式重新连接并获取其新的 deviceId/handle

    console.log('开始固件更新...');
    const update = () => {
      const reader = new FileReader();
      reader.readAsArrayBuffer(fileList.value[0].raw);
      reader.onload = async () => {
        loading.value = true;
        try {
          const res = await ServiceKeyboard.upgrade(
            reader.result,
            ({ current, total, updateStatus: status, percentage }) => {
              progress.current = current;
              progress.total = total;
              progress.percent = percentage;
              updateStatus.value = status;
            },
            {
              toBootDelay: 4000,
              toAppDelay: 4000,
            }
          );
          afterUpdate();
        } catch (error) {
          console.log('error: ', error);
          visible.value = false;
          step.value = 1;
          progress.current = 0;
        }
        loading.value = false;
      };
    };
    // 更新后设备通常会重启
  } catch (error) {
    console.error('固件更新过程中发生错误:', error);
  }
}
// const myFirmwareArrayBuffer: ArrayBuffer = ...;
// performFirmwareUpdate(myFirmwareArrayBuffer);
```

:::: danger

* 当 `updateStatus` 为 `APPNoDevice` 时，需要重新执行授权流程：依次调用 `ServiceKeyboard.getDevices()` 与 `ServiceKeyboard.init()`，确保设备重新授权成功后再继续升级。
* 当 `updateStatus` 为 `noDevice` 时，同样需要重新授权（`getDevices` + `init`），并在成功后重新走一遍固件升级流程。

::::

---

### 注意事项

::: tip

* `callback` 函数中的 `updateStatus` 参数可以用来显示当前更新的具体状态。
* 可以通过 `options` 参数调整更新过程中的各个延迟时间，以适应不同的设备需求。
:::

## 恢复出厂设置

ServiceKeyboard.GFSRestore()

**简要描述:**
将键盘恢复到出厂默认设置。此操作会清除所有用户自定义的配置，包括灯光设置、按键映射、宏设置等。

### 参数

此方法不需要参数。

### 返回值

**类型:** `Promise<void>`

**描述:** 返回一个 `Promise`，操作成功完成时解析，无特定返回值。如果操作失败，`Promise` 会拒绝并返回一个错误。

### 使用示例

```js
async function restoreFactorySettings() {
  try {
    console.log('正在尝试恢复出厂设置...');
    await ServiceKeyboard.GFSRestore();
    console.log('恢复出厂设置成功。');
    // 此时设备可能已断开连接，需要重新扫描或等待设备重新连接
  } catch (error) {
    console.error('恢复出厂设置失败:', error);
  }
}

// restoreFactorySettings();
```

---

### 注意事项

::: tip

* 恢复出厂设置会清除所有用户自定义的配置，请谨慎操作。
:::

## 获取系统类型

ServiceKeyboard.getSystemType()

**简要描述:**
获取当前键盘设置的系统类型（Windows或Mac）。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<{ key: string, value: number }>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为包含系统类型信息的对象。
* **解析对象结构:**

| 字段名称 | 类型     | 描述                     | 示例值  |
|----------|----------|--------------------------|---------|
| `key`    | `string` | 系统类型名称，可能的值为 "WIN" 或 "MAC"。 | `"MAC"` |
| `value`  | `number` | 系统类型对应的值，0 表示 Windows，1 表示 Mac。 | `1`     |

---

### 使用示例

```js
async function fetchSystemType() {
  try {
    const systemType = await ServiceKeyboard.getSystemType();
    console.log('当前系统类型:', systemType);
    // 示例输出: { "key": "MAC", "value": 1 }
  } catch (error) {
    console.error('获取系统类型失败:', error);
  }
}

// fetchSystemType();
```

## 设置系统类型

ServiceKeyboard.setSystemType(systemType)

**简要描述:**
设置键盘的系统类型（Windows或Mac）。

---

### 参数

| 参数名称 | 类型     | 描述                     | 是否必需 | 默认值 |
|----------|----------|--------------------------|----------|--------|
| `systemType` | `string` | 要设置的系统类型，只能是 "WIN" 或 "MAC"。 | 是       | 无     |

---

### 返回值

* **总体类型:** `Promise<void>`
* **描述:** 返回一个 `Promise`，操作成功完成时解析，无特定返回值。如果操作失败，`Promise` 会拒绝并返回一个错误。

---

### 使用示例

```js
async function setSystemType(systemType) {
  try {
    // systemType 只能是 "WIN" 或 "MAC"
    await ServiceKeyboard.setSystemType(systemType);
    console.log(`已成功设置系统类型为: ${systemType}`);
  } catch (error) {
    console.error('设置系统类型失败:', error);
  }
}

// 设置为 Windows 系统
// setSystemType("WIN");

// 设置为 Mac 系统
// setSystemType("MAC");
```

---

### 注意事项

::: tip

* 参数 `systemType` 只能是 "WIN" 或 "MAC"。
* 切换系统类型后，键盘的某些功能可能会有所不同，例如按键映射、修饰键布局等。
* 建议在设置系统类型后，验证键盘的功能是否正常工作。
:::
