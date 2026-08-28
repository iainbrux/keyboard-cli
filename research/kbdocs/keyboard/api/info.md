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

*   **总体类型:** `Promise<Device[]>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个 `Device` 对象数组。每个 `Device` 对象代表一个已连接的键盘设备。
*   **解析对象结构 (`Device`):**

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

*   **总体类型:** `Promise<Device | null>`
*   **描述:** 返回一个 `Promise`。
    *   **Promise 解析 (Resolves):** `Device` - 设备初始化成功时，解析为该设备的 `Device` 对象。
    *   **Promise 解析 (Resolves):** `null` - 设备初始化失败或未找到指定设备时，解析为 `null`。
*   **解析对象结构 (`Device`):**
    *   关于 `Device` 对象的详细结构，请 [查看Device的类型](/keyboard/model#设备)。

---

### 使用示例 

```js
async function initializeDevice(deviceId: string) {
  try {
    const device = await ServiceKeyboard.init(deviceId);
    if (device) {
      console.log('设备初始化成功:', device);
      // 现在可以使用 device 对象与设备进行交互
      // const deviceInfo = await ServiceKeyboard.getBaseInfo(); 
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

ServiceKeyboard.getBaseInfo()

**简要描述:**
获取当前已初始化设备的基础硬件和固件信息。

---

### 参数 

此方法不需要参数 (通常在成功调用 `ServiceKeyboard.init()` 后使用)。

---

### 返回值 

*   **总体类型:** `Promise<BaseInfo>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含设备基础信息的 `BaseInfo` 对象。
*   **解析对象结构 (`BaseInfo`):**

| 字段名称          | 类型     | 描述                     | 示例值  |
|-------------------|----------|--------------------------|---------------|
| `BoardID`         | `number` | 板卡ID。                 | `84084737`           |
| `KeyboardLayout`  | `number` | 键盘布局类型。           | `5`           |
| `KeyType`         | `number` | 按键轴体类型。           | `3`           |
| `CustomerID`      | `number` | 客户ID。                 | `0`           |
| `ProductionId`    | `number` | 产品ID。                 | `0`           |
| `KeyboardRunMode` | `number` | 键盘当前的运行模式。     | `0`           |
| `KeyboardSN`      | `string` | 键盘的序列号。           | `'123456'`          |
| `firewareSpaceSize`| `number`| 固件可用空间大小。       | `262144`           |
| `appVersion`      | `string` | 固件的应用版本号。       | `'App V1.1.6'`          |
| `appBuildDate`    | `string` | 固件的编译日期。         | `'Apr 22 5180'`          |
| `versionString`   | `string` | 包含版本信息的完整字符串。 | `''`          |

更多关于 `BaseInfo` 的细节，请 [查看BaseInfo的类型](/keyboard/model#基础信息)。

---

### 使用示例 

```js
async function fetchBaseDeviceInfo() {
  try {
    // 确保设备已初始化
    // await ServiceKeyboard.init(deviceId);
    const baseInfo = await ServiceKeyboard.getBaseInfo();
    console.log('设备基础信息:', baseInfo);
    // console.log('固件版本:', baseInfo.appVersion);
  } catch (error) {
    console.error('获取设备基础信息失败:', error);
  }
}

// fetchBaseDeviceInfo(); // 需在设备初始化后调用
```

## 获取设备API信息

ServiceKeyboard.getApi()

**简要描述:**
通过指定的类型和参数，向设备请求特定的 API 信息。

---

### 参数 

| 参数名称   | 类型                                                 | 描述                                                         | 是否必需 | 默认值      |
|------------|------------------------------------------------------|--------------------------------------------------------------|----------|-------------|
| `params`   | `object`                                             | 一个包含 API 请求参数的对象。                                  | 是       | 无          |
| `params.type` | `string`                                           | API 的类型标识符，用于指定要获取的具体信息。                 | 是       | 无          |
| `params.hArgs`| `number[]`                                         | 可选的附加参数数组，具体含义取决于 `type`。                    | 否       | `undefined` |
| `params.is8bit`| `boolean`                                        | 可选参数，指示是否按8位数据模式处理，具体含义取决于 `type`。   | 否       | `undefined` |

---

### 返回值 

*   **总体类型:** `Promise<Api>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个 `Api` 对象，其中包含请求的 API 数据。
*   **解析对象结构 (`Api`):**
    *   关于 `Api` 对象的详细结构，[查看getApi的type类型](/keyboard/type#getApi)。

---

### 使用示例 

```js
async function fetchApiData(apiType: string, customArgs?: number[], use8bit?: boolean) {
  try {
    // 确保设备已初始化
    // await ServiceKeyboard.init(deviceId);

    const apiParams: { type: string; hArgs?: number[]; is8bit?: boolean } = { type: apiType };
    if (customArgs) {
      apiParams.hArgs = customArgs;
    }
    if (typeof use8bit === 'boolean') {
      apiParams.is8bit = use8bit;
    }

    const result = await ServiceKeyboard.getApi(apiParams);
    console.log(`API 类型 ${apiType} 的结果:`, result);
  } catch (error) {
    console.error(`获取 API 类型 ${apiType} 数据失败:`, error);
  }
}

// const TYPE_GET_KEYBOARD_STATUS = "ORDER_TYPE_PROTOCOL_VERSION"; // 示例 type
// fetchApiData(TYPE_GET_KEYBOARD_STATUS);
// fetchApiData(TYPE_SOME_OTHER_API, [10, 20], true);
```

---

### 注意事项 

::: tip
*   `params.type` 的具体可用值和含义，请 [查看getApi的type类型](/keyboard/type#getApi)。
:::


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

*   **总体类型:** `Promise<void>`
*   **描述:** 返回一个 `Promise`。操作成功完成时，`Promise` 解析，无特定返回值。如果重连失败，`Promise` 会拒绝并返回一个错误。

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
*   在恢复出厂设置、固件升级等操作后，会多次触发设备的插拔事件。
*   建议在设备连接后重新获取设备信息、配置信息等数据。
*   在升级过程中的设备断开和重连需要特殊处理，确保升级流程的完整性。
*   可以通过 `data.updateFail` 判断是否在写入数据中更新失败，进行相应的错误处理。
*   在插拔事件里已经处理好了设备的自动重连，不需要再调用重连接口了
:::

## 设置回报率

ServiceKeyboard.setRateOfReturn()

**简要描述:**
设置键盘的回报率 (Polling Rate)。

---

### 参数 

| 参数名称 | 类型     | 描述                           | 是否必需 | 默认值 |
|----------|----------|--------------------------------|----------|--------|
| `value`  | `number` | 要设置的回报率值（具体单位和可选值需查阅设备文档）。 | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<void>`
*   **描述:** 返回一个 `Promise`。操作成功完成时，`Promise` 解析，无特定返回值。如果设置失败，`Promise` 会拒绝并返回一个错误。

---

### 使用示例 

```js
// keyType的值需要从ServiceKeyboard.getBaseInfo接口获取
const keyType = 3;
// 下拉框的值
const rateOfReturnList = computed(() => {
  if (keyType === 4) {
    return ['1KHz', '500Hz', '250Hz', '125Hz'];
  }
  return ['8KHz', '4KHz', '2KHz', '1KHz', '500Hz', '250Hz', '125Hz'];
});

async function configureRateOfReturn(rateValue: number) {
  try {
    // 确保设备已初始化
    // await ServiceKeyboard.init(deviceId);
    await ServiceKeyboard.setRateOfReturn(rateValue);
    console.log(`回报率已设置为: ${rateValue}`);
  } catch (error) {
    console.error('设置回报率失败:', error);
  }
}
tip:keyType的值需要从ServiceKeyboard.getBaseInfo接口获取，设置回报率 回报率最大值只有6
// const POLLING_RATE = 6; // 示例值
// configureRateOfReturn(POLLING_RATE);
```

---

### 注意事项 

::: tip
*   具体的回报率值和 `type` 的关联（如果适用），请 [查看回报率类型](/keyboard/type#getApi) (注意：此链接可能指向 `getApi` 的类型，可能需要查阅相关type定义以确定回报率的参数)。
:::



## 进入Bootloader模式

ServiceKeyboard.toBoot()

**简要描述:**
使设备进入 Bootloader 模式，通常用于擦除或准备更新固件。

---

### 参数 

此方法不需要参数。

---

### 返回值 

*   **总体类型:** `Promise<void>`
*   **描述:** 返回一个 `Promise`。操作成功完成时，`Promise` 解析，无特定返回值。如果操作失败，`Promise` 会拒绝并返回一个错误。

---

### 使用示例 

```js
async function enterBootloaderMode() {
  try {
    // 确保设备已初始化
    // await ServiceKeyboard.init(deviceId);
    console.log('正在尝试使设备进入 Bootloader 模式...');
    await ServiceKeyboard.toBoot();
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
*   执行此操作后，设备通常会断开连接并以 Bootloader 模式重新枚举（如果支持）。您可能需要重新扫描设备或使用特定的 Bootloader 工具进行后续操作（如固件更新）。
*   tip: 擦除后，需要重新连接设备,调用重连接口`ServiceKeyboard.reconnection`。
:::


## Bin文件更新固件

ServiceKeyboard.updateBin()

**简要描述:**
向处于 Bootloader 模式的设备更新固件。

---

### 参数 

| 参数名称   | 类型                                                  | 描述                                                           | 是否必需 | 默认值 |
|------------|-------------------------------------------------------|----------------------------------------------------------------|----------|--------|
| `buffer`   | `ArrayBuffer`                                         | 包含新固件二进制数据的 `ArrayBuffer`。                          | 是       | 无     |
| `callback` | `(data: {current: number, total: number, updateStatus: string}) => void` | 一个回调函数，用于报告固件更新的进度。`data.current` 是已传输的字节数，`data.total` 是总字节数，`data.updateStatus` 是当前更新状态。 | 否       | 无     |
| `options`  | `object`                                              | 更新过程中的配置选项。(可根据实际设备情况去调试)                                          | 否       | 有     |
| `options.toBootDelay` | `number`                                        | 进入 Bootloader 模式后的延迟时间（毫秒）。                      | 否       | 4000   |
| `options.writeDelay`  | `number`                                        | 写入数据时的延迟时间（毫秒）。                                  | 否       | 30     |
| `options.toAppDelay`  | `number`                                        | 返回应用模式时的延迟时间（毫秒）。                              | 否       | 4000   |

---

### 返回值 

*   **总体类型:** `Promise<{ success: boolean }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，指明固件更新是否成功。
*   **解析对象结构:**

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
          const res = await ServiceKeyboard.updateBin(
            reader.result,
            ({ current, total, updateStatus: status }) => {
              progress.current = current;
              progress.total = total;
              updateStatus.value = status;
            },
            {
              toBootDelay: 4000,
              writeDelay: 30,
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

---

### 注意事项 

::: tip
*   `callback` 函数中的 `updateStatus` 参数可以用来显示当前更新的具体状态。
*   可以通过 `options` 参数调整更新过程中的各个延迟时间，以适应不同的设备需求。
:::

## 恢复出厂设置

ServiceKeyboard.factoryDataReset()

**简要描述:**
将键盘恢复到出厂默认设置。此操作会清除所有用户自定义的配置，包括灯光设置、按键映射、宏设置等。

### 参数
此方法不需要参数。

### 返回值
**类型:** `Promise<void>`

**描述:** 返回一个 `Promise`，该 `Promise` 在操作成功时解析为 `void`。

### 使用示例
```js
async function resetToFactorySettings() {
  try {
    await ServiceKeyboard.factoryDataReset();
    console.log('已恢复出厂设置');
    
    // 恢复出厂设置后，需要重新连接设备
    // await ServiceKeyboard.reconnection(device, deviceId);
    
    // 重新获取设备信息
    // const baseInfo = await ServiceKeyboard.getBaseInfo();
    // const layoutInfo = await ServiceKeyboard.defKey();
  } catch (error) {
    console.error('恢复出厂设置失败:', error);
    throw error;
  }
}

// 示例：恢复出厂设置
// resetToFactorySettings();
```

### 注意事项
::: tip
*   此操作会清除所有用户自定义的配置，请谨慎使用。
*   恢复出厂设置后，设备会自动断开连接。
*   建议在操作完成后：
    *   重新连接设备
    *   重新获取设备基础信息
    *   重新获取键盘布局信息
    *   重新获取其他必要的配置信息
*   建议在操作过程中显示加载状态，以提供更好的用户体验。
:::

## 切换配置

ServiceKeyboard.switchConfig()

**简要描述:**
切换键盘的配置。键盘支持多个配置（通常为4个），可以通过此接口在不同配置之间切换。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `index`  | `number` | 要切换到的配置索引。有效值为1、2、3、4，分别对应四个不同的配置。                                      | 是       | 无     |

### 返回值
**类型:** `Promise<void>`

**描述:** 返回一个 `Promise`，该 `Promise` 在操作成功时解析为 `void`。

### 使用示例
```js
// 切换配置
async function switchKeyboardConfig(configIndex) {
  try {
    await ServiceKeyboard.switchConfig(configIndex);
    console.log(`已切换到配置 ${configIndex}`);
  } catch (error) {
    console.error('切换配置失败:', error);
    throw error;
  }
}

// 监听配置切换事件
ServiceKeyboard.on('getCmd', (data) => {
  console.log('getCmd', data);
  if (data && data.configID !== null && data.configID !== undefined && !appStore.loading) {
    appStore.setLoading(true, '配置切换中');
    appStore.activeConfigIndex = data.configID;
    
    // 需要重新获取配置，灯光，改键等
    // 例如：
    // await fetchNewConfig();
    // await fetchLightingSettings();
    // await fetchKeyMappings();
  }
});

// 示例：切换到配置2
// const configIndex = 2;
// switchKeyboardConfig(configIndex);
```

### 注意事项
::: tip
*   `index`: 配置索引值必须在1到4之间。
*   切换配置后，建议监听 `getCmd` 事件来获取配置切换的结果。
*   在收到配置切换成功的通知后，通常需要重新获取以下数据：
    *   键盘配置信息
    *   灯光设置
    *   按键映射
    *   其他相关设置
*   建议在配置切换过程中显示加载状态，以提供更好的用户体验。
:::


