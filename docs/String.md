# String Transformers

## StringConcatenationTransformer

Concatenates adjacent string literals in binary addition chains. Handles left-associative chaining where the left operand has already been folded.

```js
// Before
var x = "hello" + " " + "world";
var y = "a" + "b" + "c" + "d";

// After
var x = "hello world";
var y = "abcd";
```

### Combined with evaluation

When paired with the BuiltinEvaluationTransformer, string building patterns collapse:

```js
// Before
var x = String.fromCharCode(116) + String.fromCharCode(101) + String.fromCharCode(115) + String.fromCharCode(116);

// After
var x = "test";
```
