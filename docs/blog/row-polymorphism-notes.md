# Two bugs in Trestle's row polymorphism

Notes for a blog post. Two bugs turned up while building a test corpus for open records
("row types") in Trestle's type checker. Both are small in code and large in what they teach:

1. **Following tails to the end.** Resolving a record type one step at a time leaves
   already-solved tails hiding in the middle of a row.
2. **Raising the let-generalisation level.** Binding a variable to a type has to carry the
   variable's level into everything inside that type, or `let` generalises too much.

The test programs live in `crates/trestle/tests/programs/01-unification/row-polymorphism/`
and `.../let-generalisation/`.

---

## Background: rows and union-find

A closed record type lists every field: `{ x: Int, y: Int }`. An **open** record lists the
fields we know about, then a **tail** variable standing for "whatever else is there":

```
{ x: a | r }      // has an x of type a, plus the fields in r
```

Field access on something whose type we don't know yet produces an open record. In

```
let getX = (r) => r.x
```

`r` gets the type `{ x: a | r1 }`. Calling `getX({ x: 1, y: 2 })` unifies that with the closed
record `{ x: Int, y: Int }`, which solves `a := Int` and `r1 := { y: Int }`. The tail soaks up
the extra field.

Trestle's unifier is a **union-find**. Every type variable is a node. A node either points to
another node, or is a **root** that is one of:

- `FreeTypeVariable(level)`: still unknown, tagged with a let level (part 2), or
- `Concrete(type)`: solved, with the type it was solved to.

A solved type can itself contain variables, and those can be solved too. **Solving a variable
doesn't rewrite the types that mention it.** You follow the links when you need the answer.

---

## Part 1: following tails to the end

### The symptom

```
let sum4 = (r) => r.a + r.b + r.c + r.d
```

Three field reads type-check fine. The fourth panics:

```
not yet implemented: flatten records whose tail is already solved
```

### The trace

`resolve_shallow` resolved a variable by **one step**: if the variable's root was
`Concrete(t)`, it returned `t` as stored, without resolving the variables inside `t`.

```rust
Some((_, RootUnionNode::Concrete(concrete))) => concrete.clone(),
```

Here is what each read does (`t := …` means "t is now solved to …"):

| Read  | `r` resolves to | What happens | New links |
|-------|-----------------|--------------|-----------|
| `r.a` | `r` (free) | bind `r` to a fresh open record | `r := { a \| t1 }` |
| `r.b` | `{ a \| t1 }` | no `b`; `t1` is free, so bind it | `t1 := { b \| t2 }` |
| `r.c` | `{ a \| t1 }` (**stale**) | no `c`; unify `t1` with `{ c \| t3 }`. `t1` resolves to `{ b \| t2 }`: **open meets open** | `t2 := { c \| t4 }`, `t3 := { b \| t4 }` |
| `r.d` | `{ a \| t1 }` (**stale**) | no `d`; unify `t1` with `{ d \| t5 }`. `t1` resolves to `{ b \| t2 }`, **but `t2` is solved** | 💥 |

At `r.d` the open-vs-open arm asks for the let level of `t2` (it needs one to create the new
shared tail). Only free variables have levels; `t2` was solved during `r.c`. So the lookup
returns `None` and the code reaches the `todo!`.

The rule underneath: **each read only looks one link deep, and the open-vs-open step at `r.c`
creates a second link.** So the read after the first open-vs-open is the first to land on a
solved tail.

A second symptom with the same cause: re-reading a field that lives in a tail.

```
let f = (r) => r.x + r.y + r.y + r.z
```

The second `r.y` looks at `{ x | t1 }`, doesn't see `y` (it's in `t1`), and **extends the row
with another `y`** instead of reusing the one that's there. Open-vs-open unification happens to
unify the two `y`s, so the answer is still right. But it adds an empty link (`{ | t4 }`), and the
next read crashes the same way.

### The fix

Recurse instead of stopping after one step:

```rust
Some((_, RootUnionNode::Concrete(concrete))) => self.resolve_shallow(concrete),
```

This works because of how `map_free_type_variables` treats an open tail: when the mapper
returns a record for the tail variable, it merges that record's fields into the outer record
and takes over its tail. With the recursion, the mapper returns an already-flattened record:

```
{ a | t1 }  →  { a | { b | { c | t4 } } }  →  { a, b, c | t4 }
```

So every record type the unifier sees now ends in either a **free** tail or `Closed`. The
open-vs-open arm can never be handed a solved tail, and the `todo!`s can't be reached.

### The trade-off

`map_free_type_variables` walks into a record's fields as well as its tail, so the recursion
resolves *every* variable in the type, not just the tail chain. `resolve_shallow` is no longer
shallow: it behaves exactly like `resolve`. That's correct, but it means the unifier resolves
the whole subtree at each step of `unify_inner`. Keeping it truly shallow would need a separate
loop that only follows a record's tail, because the mapper is a single closure and can't tell
a field variable from a tail variable.

---

## Part 2: raising the let-generalisation level

### Background: levels

Let generalisation is what makes this work:

```
let id = (x) => x
id(1)
id("one")
```

After inferring `id`'s right-hand side, the checker **generalises**: it turns the free
variables in `(a) => a` into "for all `a`", so each use of `id` gets its own fresh copy of `a`.

But only variables that belong to *this* `let` may be generalised. A variable that something
outside can still see must stay shared:

```
let outer = (y) => {
    let constant = (x) => y     // the return type is y's variable; it belongs to outer
    ...
}
```

Trestle tracks ownership with **levels**. Each `let` right-hand side is inferred one level
deeper (`increment_let_level`), every fresh variable is stamped with the level where it was
created, and `generalise` only takes variables whose level is at least the current one. `y` was
created at `outer`'s level, which is shallower, so generalising `constant` leaves it alone.

### The symptom

This program is accepted, but `r.y` ends up as both `Int` and `String`:

```
let outer = (r) => {
    let getY = (u) => r.y
    let a: Int = getY(1)
    let b: String = getY(2)
    a
}
```

The same happens without records:

```
let outer = (f) => {
    let callF = (u) => f(0)
    let a: Int = callF(1)
    let b: String = callF(2)
    a
}
```

### Why it happens

1. `r` is created while inferring `outer`'s right-hand side: **level 1**.
2. `getY`'s right-hand side is inferred at **level 2**. Reading `r.y` creates a fresh field
   variable `α` and tail `t` at the *current* level, 2, and binds `r := { y: α | t }`.
3. Binding `r` replaces its root `FreeTypeVariable(level 1)` with `Concrete({ y: α | t })`.
   **`r`'s level 1 is thrown away.** `α` still says level 2.
4. `generalise` for `getY` (at level 2) finds `α` at level 2 and generalises it.
5. Each call to `getY` now gets a fresh `α`. One becomes `Int`, the other `String`, and
   nobody notices they were supposed to be the same field of the same `r`.

The function version is identical: calling `f` inside `callF` binds `f := (Int) => b`, with `b`
created at level 2.

The existing test `captured-variable-is-not-generalised` doesn't catch this. There the
captured type *is* `y`'s variable, still at level 1. The leak only happens when a **new**
variable is created at the inner level and then attached *inside* an outer variable's type.

### The fix

The invariant to keep is:

> A free variable's level is the **outermost** level from which it can be reached.

Once `α` is part of `r`'s type, anyone who can see `r` can see `α`, and `r` lives at level 1.
So `α` must be raised to level 1 too: "raised" in the sense your comment in `union_vars` uses,
moved outward toward the top of the program (a *smaller* number).

Trestle already does this in one place. When two free variables are unioned, the surviving
root takes the `min` of the two levels (`union_vars`). The missing case is **binding a free
variable to a concrete type**, in `union_with_concrete_type`. At that point:

1. Read the variable's level `L` *before* replacing its root.
2. Walk every free variable in the type it's being bound to (resolved).
3. Set each one's level to `min(its level, L)`.
4. Then bind the variable as before.

The occurs check already walks the same type at the same moment, so the two can share a pass.

Fixing only the field-access code, by creating `α` and `t` at `r`'s level instead of the current
one, would fix the row version but not the function version. The general fix belongs in the
binding.

A sketch (not compiled or tested):

```rust
None => {
    if self.root_occurs_in_type(root_id, concrete_type) { /* InfiniteType, as today */ }

    // Everything reachable from this variable is now visible wherever the variable is.
    let level = match root_node { RootUnionNode::FreeTypeVariable(level) => *level, _ => unreachable!() };
    self.raise_levels(concrete_type, level);

    self.set(root_id, UnionNode::RootUnionNode(RootUnionNode::Concrete(concrete_type.clone())))
        .map_err(UnifyError::FreeTypeVariableNotFoundError)
}

fn raise_levels(&mut self, ty: &Type, level: LetGeneralisationLevel) {
    let free = self.resolve(ty).fold_free_type_variables(
        &mut |var, mut acc: Vec<TypeVarId>| { acc.push(*var); acc },
        Vec::new(),
    );
    for var in free {
        if let Some((root, RootUnionNode::FreeTypeVariable(var_level))) = self.find_root(var) {
            if *var_level > level {
                let _ = self.set(root, UnionNode::RootUnionNode(RootUnionNode::FreeTypeVariable(level)));
            }
        }
    }
}
```

(Copy `root` and `var_level` out of the `find_root` borrow before calling `set`.)

Why this is enough: after the binding, every free variable inside `r`'s type carries `r`'s
level. If one of them is bound later, that binding goes through the same rule and passes the
level on again. So the invariant holds by induction over bindings.

### Tests

- `row-polymorphism/captured-row-is-not-generalised`
- `let-generalisation/captured-function-result-is-not-generalised`

Both should be rejected with a type mismatch (`Int` vs `String`). They're registered with
`ignore = "binding a variable to a type must raise the levels of the variables inside it"`
until the fix lands. Then remove the `ignore` and accept their snapshots.
