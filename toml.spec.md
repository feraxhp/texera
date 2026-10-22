# Texera TOML Syntax Specification

Language design and reference guide.

---

## 1. Introduction and Design Philosophy

In traditional text editors and terminal-based tools, syntax highlighting usually falls into two problematic extremes:

1. **Naive line-by-line regexes:** Fast, but unable to remember whether a string or multiline comment was left open on previous lines.
2. **Full AST-based parsers (like Tree-sitter):** Accurate, but heavy, hard to write, slow to compile for new languages, and expensive to keep synced in real time with incomplete code while typing.

**Texera** uses a balanced, highly efficient parsing inpired by lezer and sublime-style parsing:

* **Pushdown Automaton (PDA):** The highlighter keeps a stack of contexts (`[root, fn_body, string_body]`). Rules evaluated at any point depend strictly on the state sitting at the top of the stack.
* **Non-backtracking DFA Engine (`regex-automata`):** All regexes for a state compile together into a single Deterministic Finite Automaton with leftmost-first semantics. This guarantees linear runtime $O(N)$ and completely prevents ReDoS freezes (catastrophic backtracking).
* **Cached Line-by-Line Incremental Parsing:** Each line only needs to know the state the previous line ended with (`LexState`, an interned 4-byte handle). If you edit line 50 in a 100,000-line file, Texera only reparses from line 50 until state resyncs with the previous cache (usually 1 or 2 lines later).

---

## 2. General `.toml` File Layout

A syntax definition file in Texera splits into two main sections:

1. `[meta]`: Language metadata (name and file extension identifiers).
2. `[states.<name>]`: State definitions and their parsing rules.

```toml
[meta]
name = "my_language"
extensions = ["ml", "mlg"]

# The 'root' state is the required entry point
[states.root]
rules = [
    { match = '\bfn\b', token = "keyword.function", push = "fn_signature" },
    { match = '//.*', token = "comment" },
]

```

> [!TIP]
> **Golden Rule for Regex in TOML:**
> **Always use literal single quotes (`'...'`)** for `match`, `lookahead`, and `embed_end`.
> In TOML, double quotes (`"..."`) interpret escape sequences (`\d`, `\b`), forcing double escapes (`"\\b\\d+\\b"`). Single quotes let you write clean, natural regexes: `'\b\d+\b'`.

---

## 3. Tables and Properties Reference

### 3.1. Metadata (`[meta]`)

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `string` | Yes | Canonical language name (e.g. `"rust"`, `"json"`, `"markdown"`). |
| `extensions` | `array<string>` | No | Associated file extensions without dots (e.g. `["rs"]`, `["toml"]`). |

---

### 3.2. States (`[states.<name>]`)

Each table under `[states]` represents a mode or context the parser can be in.

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `rules` | `array<table>` | `[]` | Ordered list of parsing rules for this state. **`root` state is required.** |
| `include` | `array<string>` | `[]` | Names of other states whose rules will be inserted into the current state. Useful for sharing common blocks like comments, literals, or expressions. |
| `pop_on_newline` | `boolean` | `false` | When `true`, reaching the end of the line (`\n`) automatically pops this state. Perfect for single-line strings or contexts that shouldn't bleed into the rest of the file if unclosed. |
| `default_token` | `string` | None | Fallback token assigned to unmatched characters when no rule matches at the cursor (consecutive characters are grouped automatically). Crucial inside multiline comments and string bodies. |

---

### 3.3. Rules (`rules = [ { ... } ]`)

Each rule inside a state defines what text to match and what action to run. Rules are evaluated in strict order of appearance (first match wins).

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `match` | `string` (Regex) | **Yes** | Regular expression that must match text starting at current cursor position. **Must not match 0 bytes.** |
| `token` | `string` | No | Hierarchical highlight token (e.g. `"keyword.control"` or `"string"`). If omitted, text is consumed without highlighting (useful for skipping whitespace or silent state transitions). |
| `lookahead` | `string` (Regex) | No | Regex assertion checked immediately **after** the end of `match`, **without consuming characters**. |
| `first_line` | `boolean` | No | Only allowed in `root`. When `true`, this rule **only runs on line 0** (ideal for shebangs like `#!/usr/bin/env` or YAML frontmatter `---`). |
| `push` | `string` | No* | Pushes target state onto the stack, increasing depth. |
| `pop` | `boolean` | No* | When `true`, pops current state and returns to previous state on stack. |
| `set` | `string` | No* | Replaces current state on top of stack with another state **without increasing depth**. |
| `embed` | `string` | No* | Injects an external language (e.g. `"rust"` or capture group `"$1"`) until matching `embed_end`. |
| `embed_end` | `string` (Regex) | Conditional | Regex marking the end of embedded block. **Required if `embed` is set.** |
| `embed_end_token` | `string` | No | Specific highlight token for the closing delimiter of an embedded block. Falls back to `token` if omitted. |

> [!IMPORTANT]
> **Mutual Exclusion:** Transition actions (`push`, `pop`, `set`, and `embed`) are **mutually exclusive**. A rule can only trigger one transition at a time.

---

## 4. Mental Model: State Transitions

Understanding `push`, `pop`, and `set` is key to writing clean, reliable grammars:

```text
    PUSH                           POP                             SET (Replace)

  +--------------+                                                +--------------+
  |  new_state   | <- Top                                         |  new_state   | <- Replaces
  +--------------+                +--------------+                +--------------+
  |  prev_state  |                |  base_state  | <- New Top     |  same_parent |
  +--------------+                +--------------+                +--------------+

```

### When to use each transition

* **`push` (Nested Hierarchical Structures):**
Use when a block can contain nested blocks and needs to remember where it came from:
* Braces `{ ... }`, parentheses `( ... )`, brackets `[ ... ]`.
* Function bodies, code blocks, nestable block comments (`/* /* ... */ */`).
* String interpolation (entering `${ ... }` from inside a string).


* **`pop` (Closing Context):**
Use when hitting a delimiter that ends the current block:
* Finding `}` to close a code block.
* Finding `*/` to close a block comment.
* Finding closing quote `"` in a string.


* **`set` (Sequential State Machines):**
Use to step through phases of a single statement **without growing the stack**:
* Function definitions: `fn` (set `fn_name`) $\rightarrow$ name (set `fn_params`) $\rightarrow$ `{` (set `fn_body`).
* JSON key-value pairs: `key` (set `expect_colon`) $\rightarrow$ `:` (set `expect_value`).



---

## 5. Standard Token Taxonomy

Texera uses dot-separated (`.`) hierarchical token scopes. The theme engine resolves colors from most specific to least specific. For example, if a theme styles `"keyword"`, any `"keyword.control"` or `"keyword.function"` token inherits that style unless a more specific override exists.

### Recommended Categories

| Root Category | Standard Subcategories | Typical Usage |
| --- | --- | --- |
| `keyword` | `keyword.function`, `keyword.control`, `keyword.declaration`, `keyword.modifier`, `keyword.return`, `keyword.directive` | Reserved keywords (`fn`, `if`, `let`, `pub`, `return`, `import`). |
| `function` | `function.call`, `function.definition`, `function.builtin`, `function.method`, `function.macro` | Function and method identifiers. |
| `type` | `type.builtin`, `type.definition` | Types, classes, structs, primitives (`i32`, `String`). |
| `constant` | `constant.builtin` | Named constants or globals (`true`, `false`, `null`, `None`). |
| `variable` | `variable.parameter`, `variable.builtin`, `variable.member` | Variables and parameters. |
| `property` | — | Object keys and struct fields (`json_key`, `struct.field`). |
| `string` | `string.escape`, `string.regexp` | Literal strings and escape sequences (`\n`, `\t`). |
| `number` | `number.float`, `number.binary`, `number.hex` | Numeric literals (integers, floats, hex). |
| `boolean` | — | Boolean literals (`true`, `false`). |
| `operator` | — | Math and logic operators (`+`, `-`, `&&`, ` |
| `punctuation` | `punctuation.bracket`, `punctuation.delimiter`, `punctuation.special` | Syntactic delimiters (`{}`, `()`, `[]`, `,`, `;`, `:`, `::`). |
| `comment` | — | Line and block comments (`//`, `/* */`, `#`). |
| `markup` | `markup.heading`, `markup.bold`, `markup.italic`, `markup.raw` | Markup elements (Markdown, AsciiDoc). |

---

## 6. Practical Design Patterns

### Pattern A: Distinguishing Function Calls from Variables using `lookahead`

DFA engines without backtracking cannot run arbitrary lookaround assertions like `(?=...)` inside `match`. Use the `lookahead` property instead:

```toml
[states.expressions]
rules = [
    # 1. Match identifier ONLY if immediately followed by '(' (allows spaces)
    #    '(' is not consumed here; it will be handled by the bracket rule below
    { match = '[a-zA-Z_]\w*', lookahead = '\s*\(', token = "function.call" },

    # 2. Fall back to general variable rule if no '(' follows
    { match = '[a-zA-Z_]\w*', token = "variable" },

    # 3. Handle bracket highlighting separately
    { match = '\(', token = "punctuation.bracket", push = "parentheses" },
]

```

---

### Pattern B: Step Chaining with `set` (JSON Key-Value Pairs)

Parsing sequential tokens where an element only makes sense after a specific predecessor:

```toml
[states.object_expect_key]
rules = [
    { match = '\}', token = "punctuation.bracket", pop = true },
    # After seeing a key, wait for colon ':'
    { match = '"(?:[^"\\]|\\.)*"', token = "property", set = "object_expect_colon" },
]

[states.object_expect_colon]
rules = [
    # After seeing ':', wait for value
    { match = ':', token = "punctuation.delimiter", set = "object_expect_value" },
    { match = '\}', token = "punctuation.bracket", pop = true },
]

[states.object_expect_value]
include = ["value"]
rules = [
    # After seeing ',', expect another key
    { match = ',', token = "punctuation.delimiter", set = "object_expect_key" },
    { match = '\}', token = "punctuation.bracket", pop = true },
]

```

---

### Pattern C: Nestable Block Comments

Languages like Rust, Swift, and Kotlin allow nested block comments `/* /* ... */ */`:

```toml
[states.comments]
rules = [
    { match = '//.*', token = "comment" },
    { match = '/\*', token = "comment", push = "block_comment" },
]

[states.block_comment]
default_token = "comment"
rules = [
    # Encountering another opener pushes one more level
    { match = '/\*', token = "comment", push = "block_comment" },
    # Each closing delimiter only pops one level
    { match = '\*/', token = "comment", pop = true },
]

```

---

### Pattern D: Dynamic Language Injection with `embed`

Highlight embedded code blocks inside a host language (e.g. Markdown code fences with ````rust`):

```toml
[states.root]
rules = [
    # $1 captures target language from regex
    { match = '^```\s*([a-zA-Z0-9_-]+)', token = "punctuation.delimiter", embed = "$1", embed_end = '^```\s*$' },
    # Fallback without language: plain text until fence close
    { match = '^```\s*$', token = "punctuation.delimiter", embed = "", embed_end = '^```\s*$' },
]

```

---

### Pattern E: Line 0 Rules with `first_line`

Handles syntax valid strictly at file start:

```toml
[states.root]
rules = [
    # Shebang (e.g. #!/usr/bin/env bash)
    { match = '^#!.*', token = "keyword.directive", first_line = true },
    # Markdown YAML frontmatter (--- at file start)
    { match = '^---\s*$', token = "punctuation.delimiter", push = "yaml_frontmatter", first_line = true },
]

```

---

## 7. Full Examples

### Example 1: JSON (`syntaxes/json.toml`)

```toml
[meta]
name = "json"
extensions = ["json"]

[states.root]
include = ["value"]

[states.value]
rules = [
    { match = '\{', token = "punctuation.bracket", push = "object_expect_key" },
    { match = '\[', token = "punctuation.bracket", push = "array" },
    { match = '"(?:[^"\\]|\\.)*"', token = "string" },
    { match = '-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?', token = "number" },
    { match = '\b(?:true|false)\b', token = "boolean" },
    { match = '\bnull\b', token = "constant.builtin" },
]

[states.object_expect_key]
rules = [
    { match = '\}', token = "punctuation.bracket", pop = true },
    { match = '"(?:[^"\\]|\\.)*"', token = "property", set = "object_expect_colon" },
]

[states.object_expect_colon]
rules = [
    { match = ':', token = "punctuation.delimiter", set = "object_expect_value" },
    { match = '\}', token = "punctuation.bracket", pop = true },
]

[states.object_expect_value]
rules = [
    { match = ',', token = "punctuation.delimiter", set = "object_expect_key" },
    { match = '\}', token = "punctuation.bracket", pop = true },
]
include = ["value"]

[states.array]
rules = [
    { match = '\]', token = "punctuation.bracket", pop = true },
    { match = ',', token = "punctuation.delimiter" },
]
include = ["value"]

```

---

## 8. Common Pitfalls & FAQ

### 1. Why does the compiler warn about zero-byte matches?

Rules like `{ match = '\s*', token = "whitespace" }` can match zero characters. Since the engine must advance the cursor to avoid infinite loops, it forces a 1-byte advance, shadowing subsequent rules. Always use `+` instead of `*` (`\s+`).

### 2. How do I control whether `include` rules go before or after state rules?

Texera's parser evaluates keys in document order:

* If `include = ["comments"]` sits **above** `rules = [...]`, included rules evaluate first.
* If `include` sits **below** `rules`, local rules take precedence.
* You can also inject includes at an exact index inside the list: `rules = [ ..., { include = "comments" }, ... ]`.

### 3. What happens to memory with deeply nested structures?

Texera enforces hard limits out of the box:

* State stack depth is capped at **256 levels** (`MAX_DEPTH`). Extra `push` operations are silently ignored.
* The unique interned state pool is capped at **65,536 combinations** (`MAX_POOL_STATES`). Once full, the pool purges and caches recompile cleanly without leaking memory.