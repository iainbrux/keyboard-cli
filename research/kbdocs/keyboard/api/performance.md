# 键盘性能设置

## 获取单键的性能模式与高级键模式

ServiceKeyboard.getPerformanceMode()

**简要描述:**
获取指定按键当前的性能模式 (例如：单键触发、RT模式) 和高级键模式。

---

### 参数 

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ touchMode: string, advancedKeyMode: number }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含指定按键性能模式和高级键模式的对象。
*   **解析对象结构:**

| 字段名称        | 类型     | 描述                                                     | 示例值  |
|-----------------|----------|----------------------------------------------------------|---------------|
| `touchMode`     | `string` | 按键的性能模式。可能的值：`"single"` (单键触发), `"rt"` (RT模式), `"global"` (跟随全局设置)。 | `"single"`    |
| `advancedKeyMode`| `number` | 按键的高级键模式的数值表示。                               | `0`           |

**advancedKeyMode 的值说明:**
| 值   | 含义  | 描述 |
|------|-------|------|
| `0`  | 无    | 无特殊功能 |
| `1`  | DKS   | 双键切换 |
| `2`  | MPT   | 多次触发 |
| `3`  | MT    | 多重触发 |
| `4`  | TGL   | 切换模式 |
| `5`  | END   | 结束模式 |
| `6`  | MCR   | Macro，宏功能 |
| `8`  | SOCD  | 同时相反方向处理 |
| `9`  | RS    | 快速切换 |

---

### 使用示例 

```js
async function fetchKeyPerformanceMode(targetKeyValue: number) {
  try {
    const modeInfo = await ServiceKeyboard.getPerformanceMode(targetKeyValue);
    console.log(`按键 ${targetKeyValue} 的性能模式: ${modeInfo.touchMode}, 高级键模式: ${modeInfo.advancedKeyMode}`);
  } catch (error) {
    console.error(`获取按键 ${targetKeyValue} 性能模式失败:`, error);
  }
}

// 示例：获取 keyValue 为 80 的按键的模式
// const exampleKey = 80;
// fetchKeyPerformanceMode(exampleKey);
```

---

### 注意事项 

::: tip
*   `key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::

---

## 设置单键的性能模式与高级键模式

ServiceKeyboard.setPerformanceMode()

**简要描述:**
设置指定按键的性能模式 (例如：单击、RT模式、全局) 和高级键模式。

---

### 参数 

| 参数名称            | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|---------------------|----------|----------------------------------------------------------------------|----------|--------|
| `params`            | `object` | 一个包含设置参数的对象。                                                 | 是       | 无     |
| `params.key`        | `number` | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `params.mode`       | `string` | 要设置的性能模式。允许的值：`"single"` (单击), `"rt"` (RT模式), `"global"` (跟随全局设置)。 | 是       | 无     |
| `params.advancedKeyMode`| `number` | 要设置的高级键模式的数值。                                           | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ touchMode: string, advancedKeyMode: number }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，表示已成功应用的性能模式和高级键模式。结构与 `getPerformanceMode` 的返回值相同。
*   **解析对象结构:** (同 `getPerformanceMode` 返回值结构)

---

### 使用示例 

```js
async function applyKeyPerformanceMode(targetKeyValue: number, newMode: string, newAdvancedMode: number) {
  try {
    const params = {
      key: targetKeyValue,
      mode: newMode,
      advancedKeyMode: newAdvancedMode
    };
    const result = await ServiceKeyboard.setPerformanceMode(params);
    console.log(`按键 ${targetKeyValue} 模式已更新:`, result);
  } catch (error) {
    console.error(`设置按键 ${targetKeyValue} 模式失败:`, error);
  }
}

// 示例：将 keyValue 为 80 的按键设置为 RT 模式，高级模式为 1
// const exampleKey = 80;
// applyKeyPerformanceMode(exampleKey, "rt", 1);
```

---

### 注意事项 

::: tip
*   `params.key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::


## 获取单键首次触发行程

ServiceKeyboard.getSingleTravel()

**简要描述:**
获取指定按键在非RT模式下的首次触发行程值。

---

### 参数 

| 参数名称  | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|-----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`     | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `decimal` | `number` | 返回值的小数位数。                                                       | 否       | `2`    |

---

### 返回值 

*   **总体类型:** `Promise<number>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个数字，表示指定按键当前的首次触发行程值。

---

### 使用示例 

```js
async function fetchKeySingleTravel(targetKeyValue: number, precision?: number) {
  try {
    const travelValue = await ServiceKeyboard.getSingleTravel(targetKeyValue, precision);
    console.log(`按键 ${targetKeyValue} 的首次触发行程: ${travelValue}`);
  } catch (error) {
    console.error(`获取按键 ${targetKeyValue} 首次触发行程失败:`, error);
  }
}

// 示例：获取 keyValue 为 80 的按键的行程，保留两位小数
// const exampleKey = 80;
// fetchKeySingleTravel(exampleKey, 2);
// fetchKeySingleTravel(exampleKey); // 使用默认小数位数
```

---

### 注意事项 

::: tip
*   `key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::


## 设置单键首次触发行程

ServiceKeyboard.setSingleTravel()

**简要描述:**
设置指定按键在非RT模式下的首次触发行程值。

---

### 参数 

| 参数名称  | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|-----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`     | `number` | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `value`   | `number` | 要设置的新的行程值。                                                     | 是       | 无     |
| `decimal` | `number` | 处理行程值时使用的小数位数。                                               | 否       | `2`    |

---

### 返回值 

*   **总体类型:** `Promise<number>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个数字，表示已成功应用的行程值。

---

### 使用示例 

```js
async function applyKeySingleTravel(targetKeyValue: number, newTravelValue: number, precision?: number) {
  try {
    // 如果 precision 未提供，SDK 内部或此处调用可能会使用默认值
    const updatedTravel = await ServiceKeyboard.setSingleTravel(targetKeyValue, newTravelValue, precision);
    console.log(`按键 ${targetKeyValue} 首次触发行程已更新为: ${updatedTravel}`);
  } catch (error) {
    console.error(`设置按键 ${targetKeyValue} 首次触发行程失败:`, error);
  }
}

// 示例：设置 keyValue 为 80 的按键的行程为 1.2mm，使用默认小数位数
// const exampleKey = 80;
// applyKeySingleTravel(exampleKey, 1.2);

// 示例：设置 keyValue 为 81 的按键的行程为 1.5mm，指定3位小数
// const anotherKey = 81;
// applyKeySingleTravel(anotherKey, 1.5, 3);
```

---

### 注意事项 

::: tip
*   `key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `value`: 要设置的行程值。
*   `decimal`: 小数位数，默认为2。
:::


## 获取RT模式下的按键行程值

ServiceKeyboard.getRtTravel()

**简要描述:**
获取指定按键在RT（Rapid Trigger）模式下的按下触发行程和释放触发行程。

---

### 参数 

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{pressTravel: number, releaseTravel: number}>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含RT模式下按下和释放行程值的对象。
*   **解析对象结构:**

| 字段名称      | 类型     | 描述                 | 示例值  |
|---------------|----------|----------------------|---------------|
| `pressTravel` | `number` | RT模式下的按下触发行程。 | `0.35` (示例)  |
| `releaseTravel`| `number`| RT模式下的释放触发行程。 | `0.43` (示例)  |

---

### 使用示例 

```js
async function fetchKeyRtTravel(targetKeyValue: number) {
  try {
    const rtSettings = await ServiceKeyboard.getRtTravel(targetKeyValue);
    console.log(`按键 ${targetKeyValue} RT行程: 按下 ${rtSettings.pressTravel}, 释放 ${rtSettings.releaseTravel}`);
  } catch (error) {
    console.error(`获取按键 ${targetKeyValue} RT行程失败:`, error);
  }
}

// 示例：获取 keyValue 为 80 的按键的RT行程
// const exampleKey = 80;
// fetchKeyRtTravel(exampleKey);
```

---

### 注意事项 

::: tip
*   `key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::


## 设置RT模式下的按下触发行程

ServiceKeyboard.setRtPressTravel()

**简要描述:**
设置指定按键在RT（Rapid Trigger）模式下的按下触发行程值。

---

### 参数 

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `params` | `object` | 一个包含设置参数的对象。                                                 | 是       | 无     |
| `params.key` | `number` | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `params.value`| `number`| 要设置的RT模式下的按下触发行程值。                                       | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ pressTravel: number }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含已成功应用的RT模式按下触发行程值。
*   **解析对象结构:**

| 字段名称      | 类型     | 描述                 | 示例值  |
|---------------|----------|----------------------|---------------|
| `pressTravel` | `number` | 已设置的按下触发行程。   | `0.5` (示例)  |

---

### 使用示例 

```js
async function applyKeyRtPressTravel(targetKeyValue: number, newPressTravel: number) {
  try {
    const params = { key: targetKeyValue, value: newPressTravel };
    const result = await ServiceKeyboard.setRtPressTravel(params);
    console.log(`按键 ${targetKeyValue} RT按下行程已更新为: ${result.pressTravel}`);
  } catch (error) {
    console.error(`设置按键 ${targetKeyValue} RT按下行程失败:`, error);
  }
}

// 示例：设置 keyValue 为 80 的按键的RT按下行程为 0.4mm
// const exampleKey = 80;
// applyKeyRtPressTravel(exampleKey, 0.4);
```

---

### 注意事项 

::: tip
*   `params.key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::


## 设置RT模式下的释放触发行程

ServiceKeyboard.setRtReleaseTravel()

**简要描述:**
设置指定按键在RT（Rapid Trigger）模式下的释放触发行程值。

---

### 参数 

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `params` | `object` | 一个包含设置参数的对象。                                                 | 是       | 无     |
| `params.key` | `number` | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `params.value`| `number`| 要设置的RT模式下的释放触发行程值。                                       | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ releaseTravel: number }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含已成功应用的RT模式释放触发行程值。
*   **解析对象结构:**

| 字段名称        | 类型     | 描述                 | 示例值  |
|-----------------|----------|----------------------|---------------|
| `releaseTravel` | `number` | 已设置的释放触发行程。   | `0.2` (示例)  |

---

### 使用示例 

```js
async function applyKeyRtReleaseTravel(targetKeyValue: number, newReleaseTravel: number) {
  try {
    const params = { key: targetKeyValue, value: newReleaseTravel };
    const result = await ServiceKeyboard.setRtReleaseTravel(params);
    console.log(`按键 ${targetKeyValue} RT释放行程已更新为: ${result.releaseTravel}`);
  } catch (error) {
    console.error(`设置按键 ${targetKeyValue} RT释放行程失败:`, error);
  }
}

// 示例：设置 keyValue 为 80 的按键的RT释放行程为 0.2mm
// const exampleKey = 80;
// applyKeyRtReleaseTravel(exampleKey, 0.2);
```

---

### 注意事项 

::: tip
*   `params.key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::


## 获取单键的按压与抬起死区

ServiceKeyboard.getDpDr()

**简要描述:**
获取指定按键的按压死区 (DP) 和抬起死区 (DR) 的设置值。

---

### 参数 

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ pressDead: number, releaseDead: number }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含指定按键按压死区和抬起死区值的对象。
*   **解析对象结构:**

| 字段名称      | 类型     | 描述           | 示例值  |
|---------------|----------|----------------|---------------|
| `pressDead`   | `number` | 按键的按压死区值。 | `0.5` (示例)    |
| `releaseDead` | `number` | 按键的抬起死区值。 | `0.5` (示例)    |

---

### 使用示例 

```js
async function fetchKeyDeadZones(targetKeyValue: number) {
  try {
    const deadZones = await ServiceKeyboard.getDpDr(targetKeyValue);
    console.log(`按键 ${targetKeyValue} 死区: 按压 ${deadZones.pressDead}, 抬起 ${deadZones.releaseDead}`);
  } catch (error) {
    console.error(`获取按键 ${targetKeyValue} 死区设置失败:`, error);
  }
}

// 示例：获取 keyValue 为 80 的按键的死区设置
// const exampleKey = 80;
// fetchKeyDeadZones(exampleKey);
```

---

### 注意事项 

::: tip
*   `key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::



## 设置单键的按压死区 (DP)

ServiceKeyboard.setDp()

**简要描述:**
设置指定按键的按压死区 (DP) 值。

---

### 参数 

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`    | `number` | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `value`  | `number` | 要设置的新的按压死区值。                                               | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ pressDead: number }>` (或 `Promise<void>`，具体需确认API行为)
*   **描述:** 返回一个 `Promise`。成功时解析，可能返回包含已设置的按压死区值的对象，或无特定返回值。
*   **解析对象结构 (如果适用):**

| 字段名称    | 类型     | 描述             | 示例值  |
|-------------|----------|------------------|---------------|
| `pressDead` | `number` | 已设置的按压死区值。 | `0.5` (示例)    |

---

### 使用示例 

```js
async function applyKeyPressDeadZone(targetKeyValue: number, newDpValue: number) {
  try {
    // 假设返回更新后的值或void
    const result = await ServiceKeyboard.setDp(targetKeyValue, newDpValue);
    console.log(`按键 ${targetKeyValue} 按压死区已更新。`, result || '');
  } catch (error) {
    console.error(`设置按键 ${targetKeyValue} 按压死区失败:`, error);
  }
}

// 示例：设置 keyValue 为 80 的按键的按压死区为 0.8
// const exampleKey = 80;
// applyKeyPressDeadZone(exampleKey, 0.8);
```

---

### 注意事项 

::: tip
*   `key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `value`: 要设置的新的按压死区值。
:::



## 设置单键的抬起死区 (DR)

ServiceKeyboard.setDr()

**简要描述:**
设置指定按键的抬起死区 (DR) 值。

---

### 参数 

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`    | `number` | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `value`  | `number` | 要设置的新的抬起死区值。                                               | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ releaseDead: number }>` (或 `Promise<void>`，具体需确认API行为)
*   **描述:** 返回一个 `Promise`。成功时解析，可能返回包含已设置的抬起死区值的对象，或无特定返回值。
*   **解析对象结构 (如果适用):**

| 字段名称      | 类型     | 描述             | 示例值  |
|---------------|----------|------------------|---------------|
| `releaseDead` | `number` | 已设置的抬起死区值。 | `0.5` (示例)    |

---

### 使用示例 

```js
async function applyKeyReleaseDeadZone(targetKeyValue: number, newDrValue: number) {
  try {
    // 假设返回更新后的值或void
    const result = await ServiceKeyboard.setDr(targetKeyValue, newDrValue);
    console.log(`按键 ${targetKeyValue} 抬起死区已更新。`, result || '');
  } catch (error) {
    console.error(`设置按键 ${targetKeyValue} 抬起死区失败:`, error);
  }
}

// 示例：设置 keyValue 为 80 的按键的抬起死区为 0.8
// const exampleKey = 80;
// applyKeyReleaseDeadZone(exampleKey, 0.8);
```

---

### 注意事项 

::: tip
*   `key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `value`: 要设置的新的抬起死区值。
:::



## 行程测试

ServiceKeyboard.getRm6X21Travel()

**简要描述:**
获取特定于 RM6X21 方案的实时行程测试数据。此数据通常包含键盘上所有按键的当前触发状态和物理行程值，以二维数组的形式表示。

---

### 参数 

此方法通常不需要参数，它会返回整个键盘矩阵的行程数据。

---

### 返回值 

*   **总体类型:** `Promise<{ status: number[][], travels: number[][] }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象。此对象包含两个二维数组：`status` 和 `travels`。这些数组代表键盘按键矩阵（例如，一个6行21列的布局），分别提供每个按键的触发状态和物理行程值。
*   **解析对象结构:**

| 字段名称  | 类型        | 描述                                                                                                | 示例值 (片段)                                      |
|-----------|-------------|-----------------------------------------------------------------------------------------------------|----------------------------------------------------|
| `status`  | `number[][]`| 一个二维数组 (例如, `number[6][21]`)，表示键盘上每个按键的触发状态。内部数组的每个元素代表一个按键的状态码 (例如, `0x02` 可能表示按下状态)。 | `[[0, 2, 0, ...], [0, 0, 0, ...], ...]`            |
| `travels` | `number[][]`| 一个二维数组 (例如, `number[6][21]`)，表示键盘上每个按键的当前物理行程值。                                            | `[[10, 155, 12, ...], [8, 0, 9, ...], ...]`          |

---

### 使用示例 

```js
async function performTravelTestAndProcess() {
  try {
    const result = await ServiceKeyboard.getRm6X21Travel();
    console.log('行程测试数据:', result);
  } catch (error) {
    console.error('行程测试失败:', error);
  }
}

performTravelTestAndProcess();
```

---

### 注意事项 
::: tip
*   返回的 `status` 和 `travels` 二维数组通常直接映射到键盘的物理按键矩阵。例如，一个常见的布局可能是6行21列。
::: 


## 开始校准

ServiceKeyboard.calibrationStart

**简要描述:**
开始设备的校准流程。

---

### 参数 

此方法不需要参数。

---

### 返回值 

*   **总体类型:** `Promise<Calibration>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个 `Calibration` 对象，其中包含校准过程的状态或初始数据。
*   **解析对象结构 (`Calibration`):**
    *   关于 `Calibration` 对象的详细结构，请 [查看Calibration的类型](/keyboard/model#设备)。

---

### 使用示例 

```js
async function beginCalibration() {
  try {
    // 确保设备已初始化
    // await ServiceKeyboard.init(deviceId);
    const calibrationStatus = await ServiceKeyboard.calibrationStart();
    console.log('校准已开始:', calibrationStatus);
    // 根据 calibrationStatus 进行后续操作或UI更新
  } catch (error) {
    console.error('开始校准失败:', error);
  }
}

// beginCalibration();
```


## 结束校准

ServiceKeyboard.calibrationEnd

**简要描述:**
结束设备的校准流程。

---

### 参数 

此方法不需要参数。

---

### 返回值 

*   **总体类型:** `Promise<Calibration>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个 `Calibration` 对象，其中包含校准完成后的状态或最终数据。
*   **解析对象结构 (`Calibration`):**
    *   关于 `Calibration` 对象的详细结构，请 [查看Calibration的类型](/keyboard/model#设备)。

---

### 使用示例 

```js
const result = await ServiceKeyboard.getRm6X21Travel()

// result 返回值{ status: [] ,travels:[]}
// status数组表示当前触发的状态，什么按键触发状态，一版02表示按下状态
// travels数组表示按键触发的行程值

```

## 键盘校准

```js
const result = await ServiceKeyboard.getRm6X21Calibration()

// result 返回值{ calibrations: [] ,travels:[]}
// calibrations数组表示校准的随机值
// travels数组表示按键触发的行程值

```
