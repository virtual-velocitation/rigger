//! THE C# DEFINITIONS FIXTURE: the one source text C# DEFINITIONS ARE NODES (every C#
//! definition type is a definition of its kind) is proven over, on the symbols `CSHARP_TAGS`
//! extracts, beside the file-scoped namespace spec 119's id and `CONTAINS` tests both extract. It
//! names no crate, so every lane compiles it.

/// THE C# DEFINITIONS FIXTURE (spec 116 criterion 8): one C# file holding every C# definition
/// type. Line 1 a file-scoped namespace and 2 a block one, each of a dotted name; 3 a delegate;
/// 4 an enum of two members; 5 a record; 6 an interface holding a method; 7 a struct holding a
/// field, a `public const` and a `const` field, a `static readonly` field, a field declaration
/// naming two and a field whose initializer creates an object; 8-18 a class holding an event
/// field, an event with accessors, a property, a constructor, a destructor, an operator, a
/// conversion operator whose header is spaced wide, an indexer, and a method holding a local
/// variable, a local const, a local function and a local function with no body; 19 a class
/// holding a constructor, a destructor, an operator, a conversion operator and a method, none
/// with a body.
pub const CSHARP_DEFINITIONS_SOURCE: &str = "\
namespace S.T;
namespace A.B.C { class Probe { public Probe Go() { return this; } } }
public delegate int Fold(int a);
public enum Hue { Red = 1, Blue }
public record Rec(int X);
public interface IDraw { void Paint(); }
public struct Cell { Probe owner; public const int Max = 9; const int Min = 1; static readonly int Seed = 2; int lo, hi; Probe made = new Probe(); }
public class Foo {
  public event System.Action Changed;
  public event System.Action Moved { add { new Probe(); } remove { } }
  public Probe Size => new Probe();
  public Foo() { new Probe(); }
  ~Foo() { new Probe(); }
  public static Foo operator +(Foo a, Foo b) { return new Probe().Go(); }
  public static implicit   operator  int(Foo f) { new Probe(); return 0; }
  public int this[int i] { get { new Probe(); return i; } }
  public void Run() { var local = new Probe(); const int deep = 1; int Inner() { new Probe(); return 1; } static extern int Outer(); }
}
public abstract class Ext { extern Ext(); extern ~Ext(); public static extern Ext operator -(Ext a); public static extern explicit operator Ext(int i); public abstract void Bare(); }
";

/// `(file, source)` of a C# file-scoped namespace `N` and the class `Foo` after it (spec 119).
pub const CSHARP_FILE_SCOPED_NAMESPACE: (&str, &str) = ("s.cs", "namespace N;\nclass Foo {}\n");
