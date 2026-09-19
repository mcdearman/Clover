# quickcheck

Property-based testing for [Meadow](https://github.com/mcdearman/meadow). You
state a property, and it is checked on many random inputs. If an input breaks
it, that input is shrunk to a small one that still does.

This package is a port of Rust's
[`quickcheck`](https://github.com/BurntSushi/quickcheck) 1.0.3. It shrinks
failing inputs exactly as the crate does, and it reports failures in the
crate's words.

## Install

```sh
meadow add mcdearman/MeadowQuickcheck
```

## Use

```meadow
use Std.Random (withSeed)
use Std.Collections.Vector as V
use Std.Sort (sortBy)
use Quickcheck (forAll, forAll2, vector, int, fromBool, quickCheck, quicktest, quickcheck, failedMessage)

-- Wrong: a sorted vector's first element is not always its largest.
fun sortedFirstIsLargest xs =
  match V.head (sortBy compare xs) with
  | Just first -> fromBool (V.all (\x -> x <= first) xs)
  | None -> fromBool True

def main =
  match withSeed 1 (\() -> quicktest (quickCheck ()) (forAll (vector int) sortedFirstIsLargest)) with
  | Ok n -> "passed ${n} tests"
  | Err r -> failedMessage r
-- "[quickcheck] TEST FAILED. Arguments: ([0, 1])"

-- In a test, `quickcheck` fails the test with that message:
@test fun additionCommutes () = quickcheck (forAll2 int int (\x y -> fromBool (x + y == y + x)))
```

### Generating and shrinking

The crate uses a trait for this. Here an `Arbitrary a` is a record: a
generator, `Gen -> a`, and a shrinker, which returns a lazy `Shrinks a`.

- `int`, `int8`, `int16` and `int32`
- `uint8`, `uint16`, `uint32` and `uint64`
- `float` and `float32`
- `bool`, `char`, `string` and `unit`
- `maybe a` and `result a b`
- `vector a`, `list a` and `array a`
- `tuple2`, `tuple3` and `tuple4`

You can make your own with `arbitraryOf generate shrink`, `mapArbitrary to from
a`, and `withoutShrinking a`. The crate's shrinkers are here as
`signedShrinks least x` and `unsignedShrinks x`. `choose xs` picks one of `xs`.

Generators perform the `Std.Random` effect. If you don't handle it, the
runtime supplies real randomness, as the crate does. Wrap a run in
`withSeed n` to get the same inputs every time.

### Properties and running them

- A test returns a `TestResult`:
  - `fromBool b`, `passed` or `failed`;
  - `discard`, which counts as neither;
  - `error message`, a failure with a message;
  - `fromResult r`, for a test that gives a `Result`.
- `forAll a test` turns a test into a `Property`. `forAll2` and `forAll3` take
  a test of two or three arguments and list each argument separately when it
  fails. `forAllShown` lets you choose how arguments are written.
- `quickCheck ()` returns the crate's settings: 100 tests, at most 10,000 tries,
  and generator size 100. The `QUICKCHECK_*` environment variables change them,
  as they do for the crate, and so do `setTests`, `setMaxTests`,
  `setMinTestsPassed` and `setGen`.
- `quicktest settings property` returns `Ok` with the number of tests that
  passed, or `Err` with the first failure, shrunk. `quickcheckWith` and
  `quickcheck` fail the current `@test` instead.

Differences from the crate:

- An error inside a test is not caught. Return `error` instead.
- A generated float's power of two is a whole number, because Meadow has no
  `exp2`.

## How it's made

`src/Arbitrary.mw` and `src/Tester.mw` are hand translations of the crate. What
the crate generates depends on random numbers that cannot be seeded, so the
tests compare what does not depend on them. **`src/Cases.mw`** is generated
test data:

- the shrink sequences of integers of every width, floats (including the edges
  where converting to an integer saturates), characters, vectors, strings,
  options, results and tuples;
- 600 runs of the crate's tester on chosen failing inputs, with what each
  failure was shrunk to.

Run `scripts/generate.sh` to regenerate; it needs a Rust toolchain.

## Licence

Dual-licensed under the [Unlicense](UNLICENSE) or [MIT](LICENSE-MIT), like the
crate. See [COPYRIGHT](COPYRIGHT).
