import { Decimal } from 'decimal.js';

/**
 * 精确计算工具函数
 * @param operation 运算类型 ('add'|'subtract'|'multiply'|'divide')
 * @param num1 第一个数字
 * @param num2 第二个数字
 * @param precision 精度,小数点后位数
 * @returns 计算结果
 */
export const preciseCalculate = (
  operation: 'add' | 'subtract' | 'multiply' | 'divide',
  num1: number,
  num2: number,
  precision: number = 3,
): number => {
  const x = new Decimal(num1);
  const y = new Decimal(num2);
  let result: Decimal;

  switch (operation) {
    case 'add':
      result = x.plus(y);
      break;
    case 'subtract':
      result = x.minus(y);
      break;
    case 'multiply':
      result = x.times(y);
      break;
    case 'divide':
      result = x.dividedBy(y);
      break;
    default:
      throw new Error('不支持的运算类型');
  }

  return Number(result.toFixed(precision));
};
