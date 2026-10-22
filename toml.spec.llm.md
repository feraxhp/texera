---
name: toml-syntax-generator
description: Generates incremental syntax highlighting definitions in TOML format based on a Pushdown Automaton and non-backtracking DFA (regex-automata). Use when asked to create, design, or extend syntax highlighting support for any programming language or data format.
---

# TOML Syntax Generator (State-Stack Lexer)

You are an expert in incremental lexer design and compilers. Your goal is to write syntax definition `.toml` files for a fast syntax highlighting engine built in Rust.

## 1. Engine Architecture (Core Context)

Before writing rules, understand how the target engine processes TOML files:

1. **No AST, Pure Pushdown Automaton:** The engine tracks a context stack (`[root, fn_body, string_body]`). Which rules are active at any moment depends **strictly** on the state at the top of the stack.
2. **Line-by-Line Cached Evaluation:** Each line receives the state left by the previous line. Newlines do not drop state transitions unless a state sets `pop_on_newline = true`.
3. **DFA Execution (`regex-automata`):** All regex patterns in a state are compiled together into a single non-backtracking Deterministic Finite Automaton.
4. **First-Match Priority:** The first rule in a state's `rules` list that matches at the current byte position wins (`LeftmostFirst`).

---

## 2. TOML File Layout

### Regex Escaping Rule
**Always use literal single quotes (`'...'`)** for `match`, `lookahead`, and `embed_end`. Single quotes prevent escape processing in TOML, letting you write `'\b\d+\b'` instead of `"\\b\\d+\\b"`.

### Base Structure

```toml
[meta]
name = "language_id"
extensions = ["ext1", "ext2"]

[states.root]
# Optional: states to inline AFTER this state's own rules
include = ["comments", "expressions"]
# Optional: automatically pop this state when reaching the end of a line (\n)
pop_on_newline = false
# Optional: fallback token emitted for unmatched characters in this state
default_token = "string"

rules = [
    { match = '...', token = "..." },
]

```

### State Fields (`[states.<name>]`)

| Property | Type | Description |
| --- | --- | --- |
| `rules` | Array of tables | Ordered list of rules to test in this state. **`root` is required.** |
| `include` | Array of strings | Other state names to append to the end of `rules`. Useful for shared patterns like `comments` or `expressions`. |
| `pop_on_newline` | Boolean | If `true`, pops this state when hitting `\n`. Ideal for single-line strings or unclosed inline tokens to prevent bleed into the rest of the file. |
| `default_token` | String | Fallback token assigned to unmatched UTF-8 characters if no rule matches (adjacent matching characters merge automatically). Used inside block comments or string bodies. |

### Rule Fields (`rules = [ { ... } ]`)

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `match` | String (Regex) | **Yes** | Regex anchored implicitly to current cursor position. **Must not match 0 bytes.** |
| `token` | String | No | Hierarchical highlight token (e.g. `"keyword.control"`). If omitted, text is consumed without color (useful for whitespace or silent transitions). |
| `lookahead` | String (Regex) | No | Regex assertion checked immediately after `match` **without consuming bytes** or advancing the cursor. |
| `first_line` | Boolean | No | Allowed only in `root`. If `true`, this rule **only runs on line 0** (shebangs `#!...`, frontmatter `---`). |
| `push` | String | No* | Pushes target state onto the stack (increases depth). |
| `pop` | Boolean | No* | If `true`, pops current state off the stack back to parent state. |
| `set` | String | No* | Replaces current state at top of stack **without changing depth**. Ideal for step chains (`expect_key` -> `expect_colon` -> `expect_value`). |
| `embed` | String | No* | Target language identifier (e.g. `"rust"`) or capture group (`"$1"`) to inject until matching `embed_end`. Requires `embed_end`. |
| `embed_end` | String (Regex) | Conditional | Required if `embed` is set. Regex delimiter that closes the embedded language and returns to the host syntax. |

**Note: Transitions (`push`, `pop`, `set`, `embed`) are mutually exclusive within a single rule.*

---

## 3. Token Hierarchy Taxonomy

Colors resolve from most specific to least specific dot-separated tags (e.g. `"keyword.operator.arithmetic"` -> `"keyword.operator"` -> `"keyword"`). Use this standard taxonomy:

* **Keywords:** `keyword`, `keyword.function`, `keyword.conditional`, `keyword.repeat`, `keyword.return`, `keyword.exception`, `keyword.coroutine`, `keyword.import`, `keyword.directive`, `keyword.operator`, `keyword.modifier`, `keyword.type`, `keyword.declaration`
* **Functions:** `function`, `function.call`, `function.definition`, `function.builtin`, `function.method`, `function.macro`
* **Types and Constants:** `type`, `type.builtin`, `type.definition`, `constant`, `constant.builtin`, `constructor`
* **Variables and Properties:** `variable`, `variable.builtin`, `variable.parameter`, `variable.member`, `property`
* **Literals:** `string`, `string.escape`, `string.regexp`, `character`, `number`, `number.float`, `boolean`
* **Punctuation and Operators:** `operator`, `punctuation.bracket`, `punctuation.delimiter`, `punctuation.special`
* **Meta and Markup:** `comment`, `attribute`, `namespace`, `module`, `label`, `markup.heading`, `markup.bold`, `markup.italic`, `markup.raw`

---

## 4. Key Design Patterns

### A. Contextual Separation (Keyword scope, brackets vs values)

Never write monolithic regexes to handle structural context. Use dedicated states:

* If a keyword is only valid inside a function, **keep it out of `root**`. Move it to `fn_body`, entering `fn_body` via `push` or `set` on `{` after the signature.
* For sequential key-value structures like JSON, use `set` transitions: `object_expect_key` -> `object_expect_colon` -> `object_expect_value` -> (on `,`) `object_expect_key`. This works cleanly even across multiline formatting.

### B. Function Calls vs Variables (`lookahead`)

The DFA engine does not support inline `(?=...)` inside `match`. Distinguish calls like `my_func(x)` from `my_var` by placing the `lookahead` rule before the general identifier rule:

```toml
rules = [
    # 1. Matches identifier ONLY if followed by optional whitespace and '('
    { match = '[a-zA-Z_]\w*', lookahead = '\s*\(', token = "function.call" },
    # 2. Fallback for standard identifiers
    { match = '[a-zA-Z_]\w*', token = "variable" },
]

```

### C. Escaped Strings & Interpolation

Avoid greedy single-rule matching like `".*"`. Push a dedicated state:

```toml
[states.string_double]
pop_on_newline = true
default_token = "string"
rules = [
    # 1. Escape sequences MUST come before the closing quote rule
    { match = '\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4})', token = "string.escape" },
    # 2. Interpolation pushes code evaluation on top of the string state
    { match = '\$\{', token = "punctuation.special", push = "interpolation" },
    # 3. Closing quote pops back to the previous state
    { match = '"', token = "string", pop = true },
]

[states.interpolation]
rules = [
    { match = '\}', token = "punctuation.special", pop = true },
]
include = ["expressions"]

```

### D. Nestable Block Comments (`/* ... /* ... */ ... */`)

Push back onto the same state to manage stack depth automatically:

```toml
[states.block_comment]
default_token = "comment"
rules = [
    { match = '/\*', token = "comment", push = "block_comment" },
    { match = '\*/', token = "comment", pop = true },
]

```

---

## 5. Strict Rules & Validation Checklist

1. **NO Zero-Byte Matches:**
* **Invalid:** `match = '[0-9]*'`, `match = '.*'`, `match = '^'`, `match = '\b'`.
* **Valid:** `match = '[0-9]+'`, `match = '.+'`, `match = '^#!.*'`.
* *Why:* Patterns that match zero bytes stall cursor progression or trigger expensive bailouts. Always use `+` or `{1,}` quantifiers.


2. **No Lookarounds or Backreferences inside `match`:**
* The `regex-automata` engine guarantees linear $O(n)$ time by rejecting `(?=...)`, `(?!...)`, `(?<=...)`, and `\1`. Use the explicit `lookahead` parameter for forward assertions.


3. **Specificity Order (Most specific to least specific):**
* Long operators before short: `===`, `==`, `->`, `=>` must precede `=`, `-`, `>`, or use a sorted alternation like `match = '===|==|=>|='`.
* Floats (`\d+\.\d+`) and hex numbers (`0x[0-9a-fA-F]+`) before raw integers (`\d+`).
* Word-bounded keywords (`\b(?:if|else|for|while)\b`) before variable identifiers (`[a-zA-Z_]\w*`).


4. **Intermediate State Escape Hatches:**
* Transitional states (like `fn_signature` or `object_expect_colon`) should provide escape rules for unexpected closing delimiters (e.g. `{ match = '\}', token = "punctuation.bracket", pop = true }`) so invalid or unfinished code recovers immediately.


5. **No Cyclic `include` References:**
* If state `A` defines `include = ["B"]`, state `B` must never include `A` directly or indirectly. For recursive syntaxes (like nested parentheses), use explicit `push = "paren_expr"` transitions.



---

## 6. Full Reference Example

```toml
[meta]
name = "rust_subset"
extensions = ["rs"]

[states.root]
include = ["comments"]
rules = [
    # Shebang: only evaluated on line 0
    { match = '^#!.*', token = "keyword.directive", first_line = true },
    # Attributes like #[derive(Debug)]
    { match = '#!?\[', token = "punctuation.special", push = "attribute" },
    # Function declaration starts signature context
    { match = '\bfn\b', token = "keyword.function", push = "fn_signature" },
    { match = '\b(?:pub|struct|enum|trait|impl|mod|use|type|const|static)\b', token = "keyword" },
    { match = '\{', token = "punctuation.bracket", push = "block" },
]

[states.fn_signature]
include = ["comments"]
rules = [
    # Function name transitions immediately to awaiting parameters or body
    { match = '[a-zA-Z_]\w*', token = "function.definition", set = "fn_after_name" },
    # Recovery if user types '{' or ';' before naming the function
    { match = '\{', token = "punctuation.bracket", set = "fn_body" },
    { match = ';', token = "punctuation.delimiter", pop = true },
]

[states.fn_after_name]
include = ["comments"]
rules = [
    { match = '<', token = "punctuation.bracket", push = "generics" },
    { match = '\(', token = "punctuation.bracket", push = "fn_params" }, { match = '->', token = "keyword.operator" }, { match = '\{', token = "punctuation.bracket", set = "fn_body" }, { match = ';', token = "punctuation.delimiter", pop = true }, { match = '[A-Z]\w*', token = "type" }, ]  [states.fn_params] include = ["comments"] rules = [ { match = '\)', token = "punctuation.bracket", pop = true },
    { match = '\b(?:mut|self)\b', token = "keyword.modifier" },
    { match = '[a-zA-Z_]\w*', lookahead = '\s*:', token = "variable.parameter" },
    { match = '[:&,]', token = "punctuation.delimiter" },
    { match = '[A-Z]\w*', token = "type" },
]

# Inside function bodies, 'let' and control flow keywords are active
[states.fn_body]
include = ["comments"]
rules = [
    { match = '\{', token = "punctuation.bracket", push = "fn_body" },
    { match = '\}', token = "punctuation.bracket", pop = true },
    { match = '\blet\b', token = "keyword.declaration" },
    { match = '\b(?:mut|ref)\b', token = "keyword.modifier" },
    { match = '\b(?:if|else|match)\b', token = "keyword.conditional" },
    { match = '\b(?:for|while|loop|break|continue)\b', token = "keyword.repeat" },
    { match = '\b(?:return|yield)\b', token = "keyword.return" },
]
include = ["expressions"]

[states.block]
include = ["comments"]
rules = [
    { match = '\{', token = "punctuation.bracket", push = "block" },
    { match = '\}', token = "punctuation.bracket", pop = true },
]
include = ["expressions"]

[states.expressions]
rules = [
    { match = '"', token = "string", push = "string" },
    { match = '\b(?:true|false)\b', token = "boolean" },
    { match = '\b0x[0-9a-fA-F_]+\b|\b0b[01_]+\b|\b\d[\d_]*\.\d[\d_]*\b', token = "number.float" },
    { match = '\b\d[\d_]*\b', token = "number" },
    # Macro invocation: identifier followed by '!'
    { match = '[a-zA-Z_]\w*!', token = "function.macro" },
    # Function call: identifier followed by '(' without consuming '('
    { match = '[a-zA-Z_]\w*', lookahead = '\s*\(', token = "function.call" },
    { match = '\b[A-Z][A-Z0-9_]+\b', token = "constant" },
    { match = '\b[A-Z]\w*\b', token = "type" },
    { match = '[a-zA-Z_]\w*', token = "variable" },
    { match = '==|!=|<=|>=|&&|\|\||->|=>|\+=|-=|\*=|/=', token = "operator" },
    { match = '[+\-*/%=<>!&|^~?]', token = "operator" },
    { match = '[()\[\]]', token = "punctuation.bracket" },
    { match = '[;,.]|::|:', token = "punctuation.delimiter" },
]

[states.string]
pop_on_newline = true
default_token = "string"
rules = [
    { match = '\\.', token = "string.escape" },
    { match = '"', token = "string", pop = true },
]

[states.attribute]
rules = [
    { match = '\]', token = "punctuation.special", pop = true },
    { match = '[a-zA-Z_]\w*', token = "attribute" },
]

[states.generics]
rules = [
    { match = '<', token = "punctuation.bracket", push = "generics" },
    { match = '>', token = "punctuation.bracket", pop = true },
    { match = '[A-Z]\w*', token = "type" },
    { match = ',', token = "punctuation.delimiter" },
]

[states.comments]
rules = [
    { match = '//.*', token = "comment" },
    { match = '/\*', token = "comment", push = "block_comment" },
]

[states.block_comment]
default_token = "comment"
rules = [
    { match = '/\*', token = "comment", push = "block_comment" },
    { match = '\*/', token = "comment", pop = true },
]
```
