# Elimination Transformers

## DeadCodeTransformer

Removes unreachable and side-effect-free code from statement lists. Runs in the **Finalize** phase so that other transforms (like ControlFlowTransformer) have a chance to simplify conditions first.

Handles:
- Code after terminal statements (`return`, `throw`, `break`, `continue`)
- Side-effect-free expression statements (numeric, boolean, null, string literals, `undefined`, `void 0`)
- Empty statements
- Preserves directive prologues (`"use strict"`, `"use asm"`)

### After return

```js
// Before
function f() {
    return 1;
    var x = 2;
    x + 3;
}

// After
function f() {
    return 1;
}
```

### After throw

```js
// Before
function f() {
    throw new Error();
    var x = 2;
}

// After
function f() {
    throw new Error();
}
```

### Side-effect-free expression statements

```js
// Before
f();
3;
true;
null;
"hello";
g();

// After
f();
g();
```

### Preserves directives

```js
// Before
"use strict";
42;
f();

// After
"use strict";
f();
```

### Combined with ControlFlowTransformer

When constant conditions are simplified first, dead code elimination cleans up the result:

```js
// Before
function f() {
    if (true) { return 1; } else { return 2; }
    var x = 3;
}

// After (ControlFlowTransformer simplifies the if, then DeadCodeTransformer removes x)
function f() {
    return 1;
}
```

## UnusedVariableTransformer

Removes variable declarations that are never referenced. Works with `var`, `let`, and `const` declarations.

```js
// Before
var used = 1;
var unused = 2;
f(used);

// After
var used = 1;
f(used);
```
