# Constant Transformers

## ConstantFoldingTransformer

Folds constant expressions into their computed values at compile time. Looks through parenthesized expressions when matching operands.

### Arithmetic

```js
// Before
var x = 1 + 2;
var y = 10 - 3;
var z = 4 * 5;
var w = 20 / 4;
var r = 10 % 3;
var p = 2 ** 8;

// After
var x = 3;
var y = 7;
var z = 20;
var w = 5;
var r = 1;
var p = 256;
```

### Nested arithmetic

```js
// Before
var x = (2 + 3) * (10 - 4);

// After
var x = 30;
```

### Comparisons

```js
// Before
var a = 1 < 2;
var b = 5 === 5;
var c = 3 !== 4;

// After
var a = true;
var b = true;
var c = true;
```

### Bitwise operations

```js
// Before
var x = 0xFF & 0x0F;
var y = 1 << 4;

// After
var x = 15;
var y = 16;
```

### Boolean negation

```js
// Before
var x = !true;
var y = !false;

// After
var x = false;
var y = true;
```

### Typeof

```js
// Before
var a = typeof 42;
var b = typeof "hello";
var c = typeof true;
var d = typeof null;

// After
var a = "number";
var b = "string";
var c = "boolean";
var d = "object";
```

### Void

```js
// Before
var x = void 0;

// After
var x = undefined;
```

### Type coercion (JSFuck patterns)

Logical not on non-boolean literals follows JavaScript truthiness rules:

```js
// Before
var a = ![];
var b = !{};
var c = !0;
var d = !1;
var e = !"";
var f = !"hello";
var g = !null;

// After
var a = false;    // arrays are truthy
var b = false;    // objects are truthy
var c = true;     // 0 is falsy
var d = false;    // nonzero is truthy
var e = true;     // empty string is falsy
var f = false;    // nonempty string is truthy
var g = true;     // null is falsy
```

Unary plus coerces to number:

```js
// Before
var a = +true;
var b = +false;
var c = +null;
var d = +[];
var e = +"42";
var f = +"";

// After
var a = 1;
var b = 0;
var c = 0;
var d = 0;
var e = 42;
var f = 0;
```

Multi-pass simplification handles chained patterns:

```js
// Before (JSFuck-style)
var a = !![];        // ![] -> false, !false -> true
var b = !+[];        // +[] -> 0, !0 -> true
var c = +!![];       // !![] -> true, +true -> 1
var d = +!![] + +!![];  // -> 1 + 1 -> 2

// After
var a = true;
var b = true;
var c = 1;
var d = 2;
```

## ConstantInliningTransformer

**Status: Stub** - not yet implemented.

Will inline single-assignment constant variables into their references:

```js
// Before
const x = 5;
f(x);

// After
f(5);
```
