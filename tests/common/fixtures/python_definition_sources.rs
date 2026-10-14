//! THE PYTHON DEFINITIONS FIXTURE: the source text PYTHON DEFINITIONS ARE NODES (every Python
//! definition type is a definition of its kind) is proven over, on the symbols the Python query
//! extracts. It names no crate, so every lane compiles it.

/// THE PYTHON DEFINITIONS FIXTURE (spec 116 criterion 10). Lines 1-5 a function that calls and
/// holds a class holding a method; 6-16 a class whose body holds an annotated name with no value,
/// a tuple assignment, an assignment whose value calls, a method holding an attribute assignment
/// and a nested function that calls, and a decorated method; 17 a tuple assignment; 18 a list
/// pattern assignment with a starred target whose value calls; 19 a chained assignment whose
/// value calls; 20 a single-target assignment; 21 an annotated name with no value; 22-23 an
/// assignment under `if TYPE_CHECKING:`; 24 a generic type alias; 25-27 single-target
/// assignments whose values are an expression list, a `yield` and an augmented assignment, each
/// a value type the grammar admits; 28 a parenthesized tuple pattern assignment; 29 a plain type
/// alias.
pub const PYTHON_DEFINITIONS_SOURCE: &str = "\
def host():
    probe()
    class Local:
        def run(self):
            pass
class Shape:
    h: int
    e, g = 1, 2
    k = make()
    def area(self):
        self.x = 1
        def inner():
            paint()
    @staticmethod
    def deco():
        pass
a, b = 1, 2
[c, *d] = f()
x = y = f()
z = 1
w: int
if TYPE_CHECKING:
    t = 1
type X[T] = int
u = 1, 2
m = yield
n = o += 1
(j, q) = 1, 2
type Y = int
";
