# 手柄模式接口

## 概述

手柄模式允许将键盘模拟为游戏手柄设备，支持 Xbox 手柄模式和经典手柄模式两种类型。开启手柄模式后，键盘会以手柄设备的形式被系统识别，可以用于游戏控制等场景。

---

## 获取键盘模式信息

ServiceKeyboard.getKeyboardMode()

**简要描述:**
获取当前键盘的模式信息，包括手柄模式支持情况、当前模式状态等。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<KeyboardModeInfo>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为包含键盘模式信息的对象。
* **解析对象结构 (`KeyboardModeInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `xboxSupported` | `boolean` | 是否支持 Xbox 手柄模式 | `true` |
| `normalSupported` | `boolean` | 是否支持经典手柄模式 | `true` |
| `handleEnabled` | `boolean` | 手柄模式是否已开启 | `false` |
| `mode` | `number` | 当前模式：0-普通键盘，1-Xbox手柄，2-经典手柄 | `0` |
| `xVid` | `number` | Xbox 手柄模式的厂商 ID | `0` |
| `xPid` | `number` | Xbox 手柄模式的产品 ID | `0` |
| `nVid` | `number` | 经典手柄模式的厂商 ID | `0` |
| `nPid` | `number` | 经典手柄模式的产品 ID | `0` |
| `compatibilityMode` | `boolean` | 兼容模式是否已开启 | `false` |

**返回值示例:**

```js
{
  "xboxSupported": true,
  "normalSupported": true,
  "handleEnabled": false,
  "mode": 0,
  "xVid": 0,
  "xPid": 0,
  "nVid": 0,
  "nPid": 0,
  "compatibilityMode": false
}
```

**mode 值说明:**

| 值 | 描述 |
|-----|------|
| 0 | 普通键盘模式 |
| 1 | Xbox 手柄模式 |
| 2 | 经典手柄模式 |

---

### 使用示例

```js
async function fetchKeyboardMode() {
  const modeInfo = await ServiceKeyboard.getKeyboardMode();
  console.log('键盘模式信息:', modeInfo);
  
  // 判断是否支持手柄模式
  if (modeInfo.xboxSupported) {
    console.log('支持 Xbox 手柄模式');
  }
  if (modeInfo.normalSupported) {
    console.log('支持经典手柄模式');
  }
  
  // 判断当前模式
  if (modeInfo.handleEnabled) {
    console.log('手柄模式已开启，当前模式:', modeInfo.mode === 1 ? 'Xbox' : '经典');
  } else {
    console.log('当前为普通键盘模式');
  }
  
  return modeInfo;
}
```

---

## 开启手柄模式

ServiceKeyboard.openHandleMode(handleMode)

**简要描述:**
开启手柄模式，将键盘切换为指定的手柄类型。

---

### 参数

| 参数名称 | 类型 | 是否必需 | 描述 |
|---------|------|----------|------|
| `handleMode` | `number` | 是 | 手柄模式类型：1-Xbox手柄，2-经典手柄 |

**handleMode 值说明:**

| 值 | 描述 |
|-----|------|
| 1 | Xbox 手柄模式 |
| 2 | 经典手柄模式 |

---

### 返回值

* **总体类型:** `Promise<unknown>`
* **描述:** 操作成功解析，失败时抛出错误。

---

### 使用示例

```js
async function openHandleMode(handleMode) {
  try {
    await ServiceKeyboard.openHandleMode(handleMode);
    console.log('手柄模式已开启');
  } catch (error) {
    console.error('开启手柄模式失败:', error);
  }
}

// 示例：开启 Xbox 手柄模式
openHandleMode(1);

// 示例：开启经典手柄模式
openHandleMode(2);
```

---

## 关闭手柄模式

ServiceKeyboard.closeHandleMode()

**简要描述:**
关闭手柄模式，将键盘恢复为普通键盘模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<unknown>`
* **描述:** 操作成功解析，失败时抛出错误。

---

### 使用示例

```js
async function closeHandleMode() {
  try {
    await ServiceKeyboard.closeHandleMode();
    console.log('手柄模式已关闭');
  } catch (error) {
    console.error('关闭手柄模式失败:', error);
  }
}

closeHandleMode();
```

---

## 开启兼容模式

ServiceKeyboard.openCompatibility()

**简要描述:**
开启兼容模式。兼容模式用于解决某些系统或游戏下的兼容性问题。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<unknown>`
* **描述:** 操作成功解析，失败时抛出错误。

---

### 使用示例

```js
async function openCompatibility() {
  try {
    await ServiceKeyboard.openCompatibility();
    console.log('兼容模式已开启');
  } catch (error) {
    console.error('开启兼容模式失败:', error);
  }
}

openCompatibility();
```

---

## 关闭兼容模式

ServiceKeyboard.closeCompatibility()

**简要描述:**
关闭兼容模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<unknown>`
* **描述:** 操作成功解析，失败时抛出错误。

---

### 使用示例

```js
async function closeCompatibility() {
  try {
    await ServiceKeyboard.closeCompatibility();
    console.log('兼容模式已关闭');
  } catch (error) {
    console.error('关闭兼容模式失败:', error);
  }
}

closeCompatibility();
```

---

## 完整使用流程

### 开启手柄模式流程

```js
async function enableHandleMode(handleMode) {
  // 1. 获取当前键盘模式信息
  const modeInfo = await ServiceKeyboard.getKeyboardMode();
  
  // 2. 检查是否支持指定的手柄模式
  if (handleMode === 1 && !modeInfo.xboxSupported) {
    console.error('不支持 Xbox 手柄模式');
    return;
  }
  if (handleMode === 2 && !modeInfo.normalSupported) {
    console.error('不支持经典手柄模式');
    return;
  }
  
  // 3. 如果已经开启，先关闭
  if (modeInfo.handleEnabled) {
    await ServiceKeyboard.closeHandleMode();
    // 等待设备重新枚举
    await new Promise(resolve => setTimeout(resolve, 3000));
  }
  
  // 4. 开启手柄模式
  await ServiceKeyboard.openHandleMode(handleMode);
  
  // 5. 等待设备重新枚举（设备会断开重连）
  await new Promise(resolve => setTimeout(resolve, 3000));
  
  // 6. 重新获取设备列表并初始化
  const devices = await ServiceKeyboard.getDevices();
  if (devices.length > 0) {
    await ServiceKeyboard.init(devices[0].id);
  }
  
  // 7. 验证模式是否切换成功
  const newModeInfo = await ServiceKeyboard.getKeyboardMode();
  console.log('当前模式:', newModeInfo.mode);
  
  return newModeInfo;
}

// 示例：开启 Xbox 手柄模式
enableHandleMode(1);

// 示例：开启经典手柄模式
enableHandleMode(2);
```

### 关闭手柄模式流程

```js
async function disableHandleMode() {
  // 1. 获取当前键盘模式信息
  const modeInfo = await ServiceKeyboard.getKeyboardMode();
  
  // 2. 检查手柄模式是否已开启
  if (!modeInfo.handleEnabled) {
    console.log('手柄模式未开启');
    return;
  }
  
  // 3. 关闭手柄模式
  await ServiceKeyboard.closeHandleMode();
  
  // 4. 等待设备重新枚举
  await new Promise(resolve => setTimeout(resolve, 3000));
  
  // 5. 重新获取设备列表并初始化
  const devices = await ServiceKeyboard.getDevices();
  if (devices.length > 0) {
    await ServiceKeyboard.init(devices[0].id);
  }
  
  // 6. 验证模式是否切换成功
  const newModeInfo = await ServiceKeyboard.getKeyboardMode();
  console.log('当前模式:', newModeInfo.mode === 0 ? '普通键盘' : '手柄');
  
  return newModeInfo;
}

disableHandleMode();
```

### 切换手柄模式类型流程

```js
async function switchHandleMode(newHandleMode) {
  // 1. 获取当前键盘模式信息
  const modeInfo = await ServiceKeyboard.getKeyboardMode();
  
  // 2. 如果当前已经是目标模式，无需切换
  if (modeInfo.mode === newHandleMode && modeInfo.handleEnabled) {
    console.log('已经是目标模式');
    return modeInfo;
  }
  
  // 3. 如果手柄模式已开启但类型不同，先关闭
  if (modeInfo.handleEnabled) {
    await ServiceKeyboard.closeHandleMode();
    await new Promise(resolve => setTimeout(resolve, 3000));
  }
  
  // 4. 开启新的手柄模式
  await ServiceKeyboard.openHandleMode(newHandleMode);
  await new Promise(resolve => setTimeout(resolve, 3000));
  
  // 5. 重新初始化设备
  const devices = await ServiceKeyboard.getDevices();
  if (devices.length > 0) {
    await ServiceKeyboard.init(devices[0].id);
  }
  
  // 6. 验证切换结果
  const newModeInfo = await ServiceKeyboard.getKeyboardMode();
  return newModeInfo;
}

// 示例：从 Xbox 手柄切换到经典手柄
switchHandleMode(2);
```

### 兼容模式使用流程

```js
async function toggleCompatibility(enable) {
  // 1. 获取当前模式信息
  const modeInfo = await ServiceKeyboard.getKeyboardMode();
  
  // 2. 检查当前兼容模式状态
  if (modeInfo.compatibilityMode === enable) {
    console.log('兼容模式已经是目标状态');
    return;
  }
  
  // 3. 开启或关闭兼容模式
  if (enable) {
    await ServiceKeyboard.openCompatibility();
  } else {
    await ServiceKeyboard.closeCompatibility();
  }
  
  // 4. 等待设备重新枚举
  await new Promise(resolve => setTimeout(resolve, 3000));
  
  // 5. 重新初始化设备
  const devices = await ServiceKeyboard.getDevices();
  if (devices.length > 0) {
    await ServiceKeyboard.init(devices[0].id);
  }
  
  // 6. 验证结果
  const newModeInfo = await ServiceKeyboard.getKeyboardMode();
  console.log('兼容模式状态:', newModeInfo.compatibilityMode);
}

// 示例：开启兼容模式
toggleCompatibility(true);

// 示例：关闭兼容模式
toggleCompatibility(false);
```

---

## 注意事项

::: tip

* **设备重新枚举**: 开启/关闭手柄模式或兼容模式后，设备会断开重连，需要等待约 3 秒后重新获取设备列表并初始化。
* **模式支持检查**: 在开启手柄模式前，务必通过 `getKeyboardMode()` 检查设备是否支持目标模式。
* **HID 权限**: 切换模式后可能需要重新请求 HID 设备权限。
* **VID/PID 变化**: 开启手柄模式后，设备的 VID 和 PID 会发生变化。需要使用 `getKeyboardMode()` 返回的 `nVid` 和 `nPid`（经典手柄模式）或 `xVid` 和 `xPid`（Xbox 手柄模式）来请求轴体列表和固件升级接口。
* **兼容模式**: 兼容模式用于解决某些系统或游戏下的兼容性问题，通常在普通手柄模式无法正常工作时开启。
* **Xbox 手柄模式**: 模拟 Xbox 360 手柄，兼容性最好，支持大多数游戏。
* **经典手柄模式**: 模拟经典手柄，适用于某些特定场景。
* **按键映射**: 开启手柄模式后，需要单独配置按键映射，将键盘按键映射到手柄按键。

:::
