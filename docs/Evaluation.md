# Evaluation Transformers

## BuiltinEvaluationTransformer

Evaluates calls to known pure built-in functions when all arguments are constant. Only evaluates functions with no side effects.

### String.fromCharCode

```js
// Before
var x = String.fromCharCode(72, 101, 108, 108, 111);

// After
var x = "Hello";
```

### parseInt / Number.parseInt

```js
// Before
var a = parseInt("10");
var b = parseInt("ff", 16);
var c = Number.parseInt("10");

// After
var a = 10;
var b = 255;
var c = 10;
```

### parseFloat / Number.parseFloat

```js
// Before
var a = parseFloat("3.14");
var b = Number.parseFloat("3.14");

// After
var a = 3.14;
var b = 3.14;
```

### Number()

Type coercion to number. Handles string, boolean, and null arguments.

```js
// Before
var a = Number("42");
var b = Number("3.14");
var c = Number("");
var d = Number(true);
var e = Number(false);
var f = Number(null);

// After
var a = 42;
var b = 3.14;
var c = 0;
var d = 1;
var e = 0;
var f = 0;
```

### Boolean()

Type coercion to boolean. Follows JavaScript truthiness rules.

```js
// Before
var a = Boolean(1);
var b = Boolean(0);
var c = Boolean("hello");
var d = Boolean("");
var e = Boolean(null);

// After
var a = true;
var b = false;
var c = true;
var d = false;
var e = false;
```

### atob / btoa

Base64 decoding and encoding.

```js
// Before
var x = atob("SGVsbG8=");
var y = btoa("hello");

// After
var x = "Hello";
var y = "aGVsbG8=";
```
