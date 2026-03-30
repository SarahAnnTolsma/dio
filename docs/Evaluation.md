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

## LiteralMethodEvaluationTransformer

Evaluates method calls and property accesses on string and array literals when all arguments are constant.

### String method calls

```js
// Before
var a = "hello".charAt(0);
var b = "hello".charCodeAt(0);
var c = "hello".indexOf("ll");
var d = "hello".lastIndexOf("l");
var e = "hello".includes("ell");
var f = "hello".startsWith("he");
var g = "hello".endsWith("lo");
var h = "hello".slice(1, 3);
var i = "hello".substring(1, 3);
var j = "HELLO".toLowerCase();
var k = "hello".toUpperCase();
var l = "  hello  ".trim();
var m = "ab".repeat(3);
var n = "hello".replace("l", "r");

// After
var a = "h";
var b = 104;
var c = 2;
var d = 3;
var e = true;
var f = true;
var g = true;
var h = "el";
var i = "el";
var j = "hello";
var k = "HELLO";
var l = "hello";
var m = "ababab";
var n = "herlo";
```

### Property access on literals

```js
// Before
var a = "hello".length;
var b = "hello"[0];
var c = [1, 2, 3].length;
var d = [10, 20, 30][1];

// After
var a = 5;
var b = "h";
var c = 3;
var d = 20;
```
