# Simplification Transformers

## GlobalAliasSimplificationTransformer

Detects variables assigned to `window`, `self`, or `globalThis` that are never reassigned, and replaces member access through the alias with direct global references. The alias declaration is removed.

```js
// Before
var wn = window;
wn.Number("42");
wn.Math.ceil(1.5);
wn.document.getElementById("foo");

// After
Number("42");
Math.ceil(1.5);
document.getElementById("foo");
```

## FunctionDeclarationTransformer

Converts variable declarations with anonymous function expression initializers into function declarations. Only applies when the variable is never reassigned and the function expression has no existing name.

```js
// Before
var foo = function() { return 1; };
var add = function(a, b) { return a + b; };
var x = function named() { return 2; };  // not affected (named)

// After
function foo() { return 1; }
function add(a, b) { return a + b; }
var x = function named() { return 2; };  // unchanged
```

## BitwiseSimplificationTransformer

Simplifies complex bitwise and mixed boolean-arithmetic (MBA) expressions by evaluating them at multiple test points and matching the results against canonical operations. This handles arbitrary nesting and composition automatically — no manual pattern matching for each rewrite rule.

### XOR equivalences

```js
// Before
var x = (a & ~b) | (~a & b);
var y = (a | b) & ~(a & b);

// After
var x = a ^ b;
var y = a ^ b;
```

### De Morgan's law

```js
// Before
var x = ~(~a | ~b);
var y = ~(~a & ~b);

// After
var x = a & b;
var y = a | b;
```

### Two's complement negation

```js
// Before
var x = ~a + 1;
var y = (a ^ -1) + 1;

// After
var x = -a;
var y = -a;
```

### Addition via bitwise decomposition

```js
// Before
var x = (a ^ b) + 2 * (a & b);
var y = (a | b) + (a & b);

// After
var x = a + b;
var y = a + b;
```

### Subtraction via complement

```js
// Before
var x = a + ~b + 1;

// After
var x = a - b;
```

### Identity and constant patterns

```js
// Before
var a = ~~x;        // double NOT
var b = x ^ 0;      // XOR with 0
var c = x | 0;      // OR with 0
var d = x & -1;     // AND with all-ones
var e = x ^ x;      // XOR with self
var f = x | ~x;     // OR with complement
var g = x & ~x;     // AND with complement

// After
var a = x;
var b = x;
var c = x;
var d = x;
var e = 0;
var f = -1;
var g = 0;
```

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

Splits sequence expressions in expression statements into individual statements, and extracts leading expressions from sequence expressions in `return`, `if`, `while`, `throw`, `switch`, and `for` (test position) statements, hoisting them as standalone statements. This preserves side effects that the CommaTransformer would not handle.

Note: `do { ... } while (a, b, c);` is NOT handled because the while condition runs after the body, so hoisting would change execution order.

### Expression statements

```js
// Before
(a(), b(), c());

// After
a();
b();
c();
```

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

## VariableDeclarationSplitTransformer

Splits multi-declarator variable declarations into individual statements. This makes each declaration independent, enabling constant inlining and dead code elimination to act on them individually.

```js
// Before
var a = 1, b = 2, c = 3;

// After
var a = 1;
var b = 2;
var c = 3;
```

Preserves the declaration kind (`var`, `let`, `const`):

```js
// Before
let x = "hello", y = "world";

// After
let x = "hello";
let y = "world";
```
