# 工具模块

源码位置：`src/utils/*`

这些方法主要服务于协议组包、字节转换、颜色转换、宏动作编码和旧版发送辅助。

## bitReadWrite(value = true)

bitReadWrite(value = true)

**简要描述:**
把读写布尔值转为协议位，`true` 为读 `0x00`，`false` 为写 `0x01`。

---

### 参数

参数类型：`boolean`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = bitReadWrite(true);
```

---

### 注意事项

所属分类：字节与协议工具。

## getLightBitmap(lightSwitch, reverseEffect, superResponse)

getLightBitmap(lightSwitch, reverseEffect, superResponse)

**简要描述:**
生成灯光开关、反向效果、超级响应的 bitmap。

---

### 参数

参数类型：`boolean, boolean, boolean`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = getLightBitmap(true, false, true);
```

---

### 注意事项

所属分类：字节与协议工具。

## getSomeBits(num = 1, bit = 0x00)

getSomeBits(num = 1, bit = 0x00)

**简要描述:**
生成指定长度、指定填充值的数组。

---

### 参数

参数类型：`number, number`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = getSomeBits(4, 0xff);
```

---

### 注意事项

所属分类：字节与协议工具。

## lowByte(value)

lowByte(value)

**简要描述:**
取低 8 位。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = lowByte(0x1234);
```

---

### 注意事项

所属分类：字节与协议工具。

## highByte(value)

highByte(value)

**简要描述:**
取第 2 个字节。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = highByte(0x1234);
```

---

### 注意事项

所属分类：字节与协议工具。

## highByte16(value)

highByte16(value)

**简要描述:**
取第 3 个字节。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = highByte16(0x123456);
```

---

### 注意事项

所属分类：字节与协议工具。

## highByte24(value)

highByte24(value)

**简要描述:**
取第 4 个字节。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = highByte24(0x12345678);
```

---

### 注意事项

所属分类：字节与协议工具。

## computeHighLowByte(value)

computeHighLowByte(value)

**简要描述:**
拆成小端低/高字节。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = computeHighLowByte(1000);
```

---

### 注意事项

所属分类：字节与协议工具。

## u8ToU16(low, high)

u8ToU16(low, high)

**简要描述:**
两个 8 位小端字节合成 16 位数。

---

### 参数

参数类型：`number, number`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = u8ToU16(0xe8, 0x03);
```

---

### 注意事项

所属分类：字节与协议工具。

## calcDoubleLowToSingleHigh(doubleLow)

calcDoubleLowToSingleHigh(doubleLow)

**简要描述:**
`Uint8Array` 每两个小端字节合成 `Uint16Array`。

---

### 参数

参数类型：`Uint8Array`。

---

### 返回值

* **总体类型:** `Uint16Array`

---

### 使用示例

```ts
const result = calcDoubleLowToSingleHigh(new Uint8Array([1, 0]));
```

---

### 注意事项

所属分类：字节与协议工具。

## calcSingleHightoDoubleLow(singleHight)

calcSingleHightoDoubleLow(singleHight)

**简要描述:**
`Uint16Array` 拆成小端 `Uint8Array`。

---

### 参数

参数类型：`Uint16Array`。

---

### 返回值

* **总体类型:** `Uint8Array`

---

### 使用示例

```ts
const result = calcSingleHightoDoubleLow(new Uint16Array([256]));
```

---

### 注意事项

所属分类：字节与协议工具。

## createProtocol(data, len = 64)

createProtocol(data, len = 64)

**简要描述:**
把业务帧包装为 HID 包；超过 `len - 2` 时自动拆多包。

---

### 参数

参数类型：`number[], number?`。

---

### 返回值

* **总体类型:** `Uint8Array | Uint8Array[]`

---

### 使用示例

```ts
const result = createProtocol([0x01, 0x05]);
```

---

### 注意事项

所属分类：字节与协议工具。

## splitUint32ToUint8(value)

splitUint32ToUint8(value)

**简要描述:**
32 位数拆成大端 4 字节。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = splitUint32ToUint8(0x12345678);
```

---

### 注意事项

所属分类：字节与协议工具。

## splitUint8ToUint32LittleEndian(value)

splitUint8ToUint32LittleEndian(value)

**简要描述:**
32 位数拆成小端 4 字节。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = splitUint8ToUint32LittleEndian(0x12345678);
```

---

### 注意事项

所属分类：字节与协议工具。

## combineUint8ToUint32(a, b, c, d)

combineUint8ToUint32(a, b, c, d)

**简要描述:**
4 个字节合成 32 位数。

---

### 参数

参数类型：`number, number, number, number`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = combineUint8ToUint32(0x12, 0x34, 0x56, 0x78);
```

---

### 注意事项

所属分类：字节与协议工具。

## splitUint24ToUint8(value)

splitUint24ToUint8(value)

**简要描述:**
24 位数拆成 3 字节。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = splitUint24ToUint8(0x123456);
```

---

### 注意事项

所属分类：字节与协议工具。

## combineUint8ToUint24(a, b, c)

combineUint8ToUint24(a, b, c)

**简要描述:**
3 个字节合成 24 位数。

---

### 参数

参数类型：`number, number, number`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = combineUint8ToUint24(0x12, 0x34, 0x56);
```

---

### 注意事项

所属分类：字节与协议工具。

## convertTwoBytesToThreeSegments(data)

convertTwoBytesToThreeSegments(data)

**简要描述:**
每 2 字节转换为 row、col、ratio 三段布局数据。

---

### 参数

参数类型：`Uint8Array`。

---

### 返回值

* **总体类型:** `Uint8Array`

---

### 使用示例

```ts
const result = convertTwoBytesToThreeSegments(data);
```

---

### 注意事项

所属分类：字节与协议工具。

## calculateCRC32(bytes)

calculateCRC32(bytes)

**简要描述:**
计算 CRC32 并返回低 16 位数值。

---

### 参数

参数类型：`number[] | Uint8Array`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = calculateCRC32([0x01, 0x05]);
```

---

### 注意事项

所属分类：字节与协议工具。

## packageHead(params)

packageHead(params)

**简要描述:**
按软件帧格式拼接帧头、长度、CRC 和数据。

---

### 参数

参数类型：`{ crcData, distinguishHeader, len }`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = packageHead({ crcData, distinguishHeader: 0x55aa, len: 1 });
```

---

### 注意事项

所属分类：字节与协议工具。

## toNumber(value)

toNumber(value)

**简要描述:**
空值或 0 归一为 0，否则返回原值。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = toNumber(value);
```

---

### 注意事项

所属分类：字节与协议工具。

## computeHighLowByteToNumber(value)

computeHighLowByteToNumber(value)

**简要描述:**
空值或 0 返回 `[0, 0]`，否则返回小端低/高字节。

---

### 参数

参数类型：`number`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = computeHighLowByteToNumber(1000);
```

---

### 注意事项

所属分类：字节与协议工具。

## getKeyByValue(obj, value)

getKeyByValue(obj, value)

**简要描述:**
根据枚举/对象值反查 key。

---

### 参数

参数类型：`object, value`。

---

### 返回值

* **总体类型:** `key | undefined`

---

### 使用示例

```ts
const result = getKeyByValue(GlobalFeatureRateOfReturn, 1000);
```

---

### 注意事项

所属分类：枚举与布局工具。

## getEnumValueByKey(enumType, key)

getEnumValueByKey(enumType, key)

**简要描述:**
根据 key 读取枚举值。

---

### 参数

参数类型：`object, key`。

---

### 返回值

* **总体类型:** `value | undefined`

---

### 使用示例

```ts
const result = getEnumValueByKey(GlobalFeatureRateOfReturn, 'R1KHz');
```

---

### 注意事项

所属分类：枚举与布局工具。

## getFormatLabel(enumObj, value)

getFormatLabel(enumObj, value)

**简要描述:**
根据枚举值获取枚举 key，不存在时抛错。

---

### 参数

参数类型：`enum, number`。

---

### 返回值

* **总体类型:** `keyof enum`

---

### 使用示例

```ts
const result = getFormatLabel(GlobalFeatureRateOfReturn, 1000);
```

---

### 注意事项

所属分类：枚举与布局工具。

## keycodeToLayout(keycodes)

keycodeToLayout(keycodes)

**简要描述:**
把 keycode 数组转换为布局数组。

---

### 参数

参数类型：`Uint16Array`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = keycodeToLayout(keycodes);
```

---

### 注意事项

所属分类：枚举与布局工具。

## keycodeToLayoutStyle(keyStyle)

keycodeToLayoutStyle(keyStyle)

**简要描述:**
每 3 字节转换为 `{ row, col, ratio, type }`。`ratio === 12` 时类型为 `knob`。

---

### 参数

参数类型：`Uint8Array`。

---

### 返回值

* **总体类型:** `Array<{ row; col; ratio; type }>`

---

### 使用示例

```ts
const result = keycodeToLayoutStyle(styleBytes);
```

---

### 注意事项

所属分类：枚举与布局工具。

## buildMacroAction(status, delay, keyCode)

buildMacroAction(status, delay, keyCode)

**简要描述:**
把宏动作编码为 4 字节小端数据。`delay` 不能超过 `0x7fff`，`keyCode` 不能超过 `0xffff`。

---

### 参数

参数类型：`number, number, number`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = buildMacroAction(1, 20, 0x0004);
```

---

### 注意事项

所属分类：宏工具。

## decodeMacroBytesArrayLE(bytes)

decodeMacroBytesArrayLE(bytes)

**简要描述:**
把 4 字节小端宏动作解码为 `{ status, delay, keyCode }`。

---

### 参数

参数类型：`number[]`。

---

### 返回值

* **总体类型:** `{ status, delay, keyCode }`

---

### 使用示例

```ts
const result = decodeMacroBytesArrayLE([4, 0, 20, 128]);
```

---

### 注意事项

所属分类：宏工具。

## hexToRgb(hex)

hexToRgb(hex)

**简要描述:**
十六进制颜色转 RGB 对象。

---

### 参数

参数类型：`string`。

---

### 返回值

* **总体类型:** `{ r, g, b } | null`

---

### 使用示例

```ts
const result = hexToRgb('#FF00AA');
```

---

### 注意事项

所属分类：颜色工具。

## rgbToHex(r, g, b)

rgbToHex(r, g, b)

**简要描述:**
RGB 数值转十六进制颜色字符串。

---

### 参数

参数类型：`number, number, number`。

---

### 返回值

* **总体类型:** `string`

---

### 使用示例

```ts
const result = rgbToHex(255, 0, 170);
```

---

### 注意事项

所属分类：颜色工具。

## hexToRgbArray(hex)

hexToRgbArray(hex)

**简要描述:**
十六进制颜色转 RGB 数组。

---

### 参数

参数类型：`string`。

---

### 返回值

* **总体类型:** `number[] | null`

---

### 使用示例

```ts
const result = hexToRgbArray('#FF00AA');
```

---

### 注意事项

所属分类：颜色工具。

## getLittleEndianColorArray(hexColor, format)

getLittleEndianColorArray(hexColor, format)

**简要描述:**
颜色字符串按灯光格式转小端字节。

---

### 参数

参数类型：`string, LightingDataMode`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = getLittleEndianColorArray('#FF00AA', LightingDataMode.RGB888_U24);
```

---

### 注意事项

所属分类：颜色工具。

## getLittleEndianRgbArray(r, g, b, format)

getLittleEndianRgbArray(r, g, b, format)

**简要描述:**
RGB 数值按灯光格式转小端字节。

---

### 参数

参数类型：`number, number, number, LightingDataMode`。

---

### 返回值

* **总体类型:** `number[]`

---

### 使用示例

```ts
const result = getLittleEndianRgbArray(255, 0, 170, LightingDataMode.RGB888_U24);
```

---

### 注意事项

所属分类：颜色工具。

## u32ToHex(bytes)

u32ToHex(bytes)

**简要描述:**
U32 小端颜色字节转 hex。

---

### 参数

参数类型：`number[]`。

---

### 返回值

* **总体类型:** `string`

---

### 使用示例

```ts
const result = u32ToHex([0xaa, 0x00, 0xff, 0x00]);
```

---

### 注意事项

所属分类：颜色工具。

## u24ToHex(bytes)

u24ToHex(bytes)

**简要描述:**
U24 小端颜色字节转 hex。

---

### 参数

参数类型：`number[]`。

---

### 返回值

* **总体类型:** `string`

---

### 使用示例

```ts
const result = u24ToHex([0xaa, 0x00, 0xff]);
```

---

### 注意事项

所属分类：颜色工具。

## u16ToHex(bytes)

u16ToHex(bytes)

**简要描述:**
RGB565 U16 小端颜色字节转 hex。

---

### 参数

参数类型：`number[]`。

---

### 返回值

* **总体类型:** `string`

---

### 使用示例

```ts
const result = u16ToHex([0x1f, 0xf8]);
```

---

### 注意事项

所属分类：颜色工具。

## u8ToHex(bytes)

u8ToHex(bytes)

**简要描述:**
单色灰度字节转 hex。

---

### 参数

参数类型：`number[]`。

---

### 返回值

* **总体类型:** `string`

---

### 使用示例

```ts
const result = u8ToHex([128]);
```

---

### 注意事项

所属分类：颜色工具。

## pixelDataToHex(bytes, format)

pixelDataToHex(bytes, format)

**简要描述:**
根据灯光格式把像素字节转 hex。

---

### 参数

参数类型：`number[], LightingDataMode`。

---

### 返回值

* **总体类型:** `string`

---

### 使用示例

```ts
const result = pixelDataToHex(bytes, LightingDataMode.RGB888_U24);
```

---

### 注意事项

所属分类：颜色工具。

## pixelDataToRgb(bytes, format)

pixelDataToRgb(bytes, format)

**简要描述:**
当前实现等同于 `pixelDataToHex`，返回 hex 字符串或 `null`。

---

### 参数

参数类型：`number[], LightingDataMode`。

---

### 返回值

* **总体类型:** `string | null`

---

### 使用示例

```ts
const result = pixelDataToRgb(bytes, LightingDataMode.RGB888_U24);
```

---

### 注意事项

所属分类：颜色工具。

## preciseCalculate(operation, num1, num2, precision = 3)

preciseCalculate(operation, num1, num2, precision = 3)

**简要描述:**
基于 `decimal.js` 做精确加减乘除。

---

### 参数

参数类型：`'add' | 'subtract' | 'multiply' | 'divide', number, number, number?`。

---

### 返回值

* **总体类型:** `number`

---

### 使用示例

```ts
const result = preciseCalculate('multiply', 0.1, 0.2, 4);
```

---

### 注意事项

所属分类：精确计算工具。

## createProtocolHelper(deviceBase)

createProtocolHelper(deviceBase)

**简要描述:**
创建 `ProtocolHelper` 实例。

---

### 参数

参数类型：`DeviceBase`。

---

### 返回值

* **总体类型:** `ProtocolHelper`

---

### 使用示例

```ts
const helper = createProtocolHelper(deviceBase);
```

---

### 注意事项

所属分类：旧版协议发送辅助。

## ProtocolHelper.sendProtocol(buildProtocol, parseResult?, options?)

ProtocolHelper.sendProtocol(buildProtocol, parseResult?, options?)

**简要描述:**
发送单个或多个协议包，取首个响应并可解析。

---

### 参数

参数类型：`() => Uint8Array | Uint8Array[], parser?, { timeout? }?`。

---

### 返回值

* **总体类型:** `Promise<T | null>`

---

### 使用示例

```ts
await helper.sendProtocol(build, parse);
```

---

### 注意事项

所属分类：旧版协议发送辅助。

## ProtocolHelper.sendProtocolMulti(buildProtocol, parseResult?, options?)

ProtocolHelper.sendProtocolMulti(buildProtocol, parseResult?, options?)

**简要描述:**
发送多个协议包并解析多个响应。

---

### 参数

参数类型：`() => Uint8Array[], parser?, { timeout? }?`。

---

### 返回值

* **总体类型:** `Promise<T[]>`

---

### 使用示例

```ts
await helper.sendProtocolMulti(build);
```

---

### 注意事项

所属分类：旧版协议发送辅助。

## ProtocolHelper.sendProtocolMultiResponse(buildProtocol, parseResult?, options)

ProtocolHelper.sendProtocolMultiResponse(buildProtocol, parseResult?, options)

**简要描述:**
发送单个协议包并等待多个响应。

---

### 参数

参数类型：`() => Uint8Array, parser?, { expectedResponses, timeout? }`。

---

### 返回值

* **总体类型:** `Promise<T[]>`

---

### 使用示例

```ts
await helper.sendProtocolMultiResponse(build, parse, { expectedResponses: 2 });
```

---

### 注意事项

所属分类：旧版协议发送辅助。

## ProtocolHelper.sendProtocolNoResponse(buildProtocol)

ProtocolHelper.sendProtocolNoResponse(buildProtocol)

**简要描述:**
发送协议包且不等待响应。

---

### 参数

参数类型：`() => Uint8Array`。

---

### 返回值

* **总体类型:** `Promise<void>`

---

### 使用示例

```ts
await helper.sendProtocolNoResponse(build);
```

---

### 注意事项

所属分类：旧版协议发送辅助。

## sendProtocol(deviceBase, buildProtocol, parseResult?, options?)

sendProtocol(deviceBase, buildProtocol, parseResult?, options?)

**简要描述:**
函数式单包发送辅助。

---

### 参数

参数类型：`DeviceBase, () => Uint8Array, parser?, { timeout? }?`。

---

### 返回值

* **总体类型:** `Promise<T | null>`

---

### 使用示例

```ts
await sendProtocol(deviceBase, build, parse);
```

---

### 注意事项

所属分类：旧版协议发送辅助。

## sendProtocolNoResponse(deviceBase, buildProtocol)

sendProtocolNoResponse(deviceBase, buildProtocol)

**简要描述:**
函数式无响应发送辅助。

---

### 参数

参数类型：`DeviceBase, () => Uint8Array`。

---

### 返回值

* **总体类型:** `Promise<void>`

---

### 使用示例

```ts
await sendProtocolNoResponse(deviceBase, build);
```

---

### 注意事项

所属分类：旧版协议发送辅助。
