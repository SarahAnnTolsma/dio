# Simplification Transformers

## BlockNormalizationTransformer

Wraps bare statements in control flow bodies with block statements. Preserves `else if` chains.

```js
// Before
if (x) foo();
if (x) foo(); else bar();
if (x) foo(); else if (y) bar();
while (x) foo();
for (var i = 0; i < 10; i++) foo();
for (var k in obj) foo();
for (var v of arr) foo();
do foo(); while (x);

// After
if (x) { foo(); }
if (x) { foo(); } else { bar(); }
if (x) { foo(); } else if (y) { bar(); }
while (x) { foo(); }
for (var i = 0; i < 10; i++) { foo(); }
for (var k in obj) { foo(); }
for (var v of arr) { foo(); }
do { foo(); } while (x);
```

## CommaTransformer

Removes side-effect-free leading expressions from sequence (comma) expressions. Only drops expressions that are guaranteed to have no side effects: literals and identifiers.

```js
// Before
var x = (1, 2, 3);
var y = (a, "hello", getValue());

// After
var x = 3;
var y = getValue();
```

## ControlFlowTransformer

Simplifies if/else statements and ternary expressions when the condition is a known constant value. Evaluates boolean, numeric, string, and null literals as conditions. Unwraps single-statement blocks when replacing if statements.

### If/else with constant condition

```js
// Before
if (true) { x = 1; } else { x = 2; }
if (false) { x = 1; } else { x = 2; }
if (false) { x = 1; }

// After
x = 1;
x = 2;
// (removed entirely)
```

### Ternary with constant condition

```js
// Before
var x = true ? "yes" : "no";
var y = 0 ? "yes" : "no";
var z = "" ? "yes" : "no";
var w = null ? "yes" : "no";

// After
var x = "yes";
var y = "no";
var z = "no";
var w = "no";
```

## MemberTransformer

Converts computed member expressions with string literal keys to dot notation when the key is a valid JavaScript identifier. Rejects reserved words and invalid identifier characters.

```js
// Before
obj["property"];
obj["hello world"];
obj["class"];
obj["0"];

// After
obj.property;
obj["hello world"];   // kept: contains space
obj["class"];          // kept: reserved word
obj["0"];              // kept: starts with digit
```

## LogicalToIfTransformer

Converts standalone logical `&&` and `||` expressions (used as statements, not values) into if statements.

```js
// Before
x && y();
x || y();
var z = x && y;  // not affected (value position)

// After
if (x) { y(); }
if (!x) { y(); }
var z = x && y;  // unchanged
```

### Combined with ControlFlowTransformer

When the left side is a constant, the logical expression is first converted to an if statement, then simplified:

```js
// Before
true && console.log("hi");

// After (intermediate: if (true) { console.log("hi"); })
console.log("hi");
```

## SequenceStatementTransformer

Extracts leading expressions from sequence expressions in `return`, `if`, `while`, `throw`, `switch`, and `for` (test position) statements, hoisting them as standalone statements. This preserves side effects that the CommaTransformer would not handle.

Note: `do { ... } while (a, b, c);` is NOT handled because the while condition runs after the body, so hoisting would change execution order.

### Return statements

```js
// Before
function f() {
    return (a(), b(), c());
}

// After
function f() {
    a();
    b();
    return c();
}
```

### If statements

```js
// Before
if (a(), b(), c) { x(); }

// After
a();
b();
if (c) { x(); }
```

### While statements

```js
// Before
while ((a(), b(), c)) { x(); }

// After
a();
b();
while (c) { x(); }
```

### Throw statements

```js
// Before
throw (a(), b(), c);

// After
a();
b();
throw c;
```

### Switch statements

```js
// Before
switch ((a(), b(), x)) { case 1: break; }

// After
a();
b();
switch (x) { case 1: break; }
```

### For statement test

```js
// Before
for (; (a(), b(), c); ) { x(); }

// After
a();
b();
for (; c;) { x(); }
```

## TernaryToIfTransformer

Converts standalone ternary expressions (used as statements, not values) into if/else statements. Does not affect ternaries used as values in assignments, return statements, etc.

```js
// Before
x ? y() : z();
condition ? a = 1 : a = 2;
var v = a ? b : c;  // not affected

// After
if (x) { y(); } else { z(); }
if (condition) { a = 1; } else { a = 2; }
var v = a ? b : c;  // unchanged
```

### Combined with ControlFlowTransformer

When the condition is constant, the ternary is first converted to if/else, then simplified:

```js
// Before
true ? y() : z();

// After (intermediate: if (true) { y(); } else { z(); })
y();
```
