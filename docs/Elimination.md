# Elimination Transformers

## DeadCodeTransformer

Removes unreachable code after terminal statements (`return`, `throw`, `break`, `continue`) in any statement list. Also removes empty statements. Operates on all statement lists including function bodies, block bodies, and program bodies.

Runs in the **Finalize** phase so that other transforms (like ControlFlowTransformer) have a chance to simplify conditions first.

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
