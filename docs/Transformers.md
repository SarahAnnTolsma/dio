# Transformers

dio applies a series of AST transformers to simplify and deobfuscate JavaScript. Transformers run in two phases:

- **Main** phase runs in a convergence loop until no transformer makes changes.
- **Finalize** phase runs once after the main loop converges. If it makes changes, the main loop restarts.

## Transformer Groups

### [Constant](Constant.md)

Transformers that evaluate or propagate constant values.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| ConstantFoldingTransformer | Active | Main | Folds constant arithmetic, comparisons, typeof, and void expressions |
| ConstantInliningTransformer | Stub | Main | Inlines single-assignment constants into their references |

### [Evaluation](Evaluation.md)

Transformers that evaluate known built-in function calls.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| BuiltinEvaluationTransformer | Active | Main | Evaluates pure built-in functions with constant arguments |

### [String](String.md)

Transformers that simplify string operations.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| StringConcatenationTransformer | Active | Main | Concatenates adjacent string literal additions |

### [Simplification](Simplification.md)

Transformers that normalize and simplify control flow and expressions.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| BlockNormalizationTransformer | Active | Main | Wraps bare control flow bodies in block statements |
| CommaTransformer | Active | Main | Removes side-effect-free leading expressions from sequences |
| ControlFlowTransformer | Active | Main | Simplifies if/else and ternaries with constant conditions |
| MemberTransformer | Active | Main | Converts computed member access to dot notation |
| SequenceStatementTransformer | Active | Main | Hoists leading expressions from sequences in return/if |
| TernaryToIfTransformer | Active | Main | Converts standalone ternary expressions to if/else |

### [Elimination](Elimination.md)

Transformers that remove dead or unreachable code.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| DeadCodeTransformer | Active | Finalize | Removes unreachable code after return/throw/break/continue |

### [Renaming](Renaming.md)

Transformers that rename obfuscated identifiers.

| Transformer | Status | Phase | Description |
|---|---|---|---|
| VariableRenamingTransformer | Stub | Finalize | Renames obfuscated variable names to short readable names |
