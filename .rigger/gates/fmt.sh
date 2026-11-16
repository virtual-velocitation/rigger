#!/bin/sh
# The fmt gate: `rustfmt --check` over every tracked Rust file, each by its own crate's edition.
#
# Why not `cargo fmt --all --check`: cargo fmt names each target's root file to rustfmt, and
# rustfmt reaches every other file by walking the `mod` declarations from there. Some rustfmt
# releases do not walk into a module declared inside a macro such as `std::cfg_select!`, which is
# how this workspace declares its per-OS adapters, so on such a toolchain a mis-formatted adapter
# passed the gate and then failed CI's newer rustfmt. Naming every tracked `.rs` file to rustfmt
# checks each one whether or not a walk reaches it. rustfmt still walks from each file it is
# named, so every file cargo fmt's walk reaches is checked too, and this one check replaces it.
#
# The edition: rustfmt's style follows the edition it is given, and cargo fmt gives each target
# its own crate's. Here each file takes the edition of the nearest tracked Cargo.toml at or above
# it that declares a [package]: its `edition = "<year>"`, or 2015, cargo's default, when it
# declares none. An edition declared in any other form (`edition.workspace = true`) is refused,
# never guessed, and so is a Rust file that lies in no crate.
#
# Excluded files: none. Every tracked `.rs` file is standalone Rust; none is an `include!`
# fragment or a fixture kept unformatted on purpose. Such a file would be excluded by its path,
# in a list here giving the reason for each.
#
# Usage: sh .rigger/gates/fmt.sh   (from the worktree root)

# Each crate, as one `<edition> <directory>` line per tracked Cargo.toml that declares a
# [package]; the root's directory is `.`.
crates=$(git ls-files -- Cargo.toml '*/Cargo.toml' | awk '
    {
        manifest = $0
        dir = manifest
        sub(/\/?Cargo\.toml$/, "", dir)
        if (dir == "") dir = "."
        package = 0
        crate = 0
        edition = 2015
        while ((getline line < manifest) > 0) {
            if (line ~ /^[ \t]*\[/) {
                package = line ~ /^[ \t]*\[[ \t]*package[ \t]*\][ \t]*(#.*)?$/
                crate = crate || package
            } else if (package && line ~ /^[ \t]*edition[ \t]*[.=]/) {
                if (line !~ /^[ \t]*edition[ \t]*=[ \t]*"[0-9]+"[ \t]*(#.*)?$/) {
                    print "fmt gate: " manifest " declares its edition as `" line "`; only edition = \"<year>\" is read, so the gate refuses rather than guess" > "/dev/stderr"
                    exit 1
                }
                edition = line
                sub(/^[^"]*"/, "", edition)
                sub(/".*/, "", edition)
            }
        }
        close(manifest)
        if (crate) print edition, dir
    }') || exit 1

# Each tracked Rust file, as one `<edition> <path>` line: the edition of the deepest crate
# directory that holds it.
files=$(git ls-files -- '*.rs' | crates=$crates awk '
    BEGIN {
        n = split(ENVIRON["crates"], row, "\n")
        for (i = 1; i <= n; i++) {
            at = index(row[i], " ")
            edition[i] = substr(row[i], 1, at - 1)
            dir[i] = substr(row[i], at + 1)
        }
    }
    {
        best = 0
        for (i = 1; i <= n; i++)
            if ((dir[i] == "." || index($0, dir[i] "/") == 1) && (!best || length(dir[i]) > length(dir[best])))
                best = i
        if (!best) {
            print "fmt gate: " $0 " lies in no crate, so it has no edition to be checked by; refusing" > "/dev/stderr"
            exit 1
        }
        print edition[best], $0
    }') || exit 1

# One rustfmt per edition, over at most 100 files a call so a command line stays under the
# Windows length limit. rustfmt prints a diff naming each mis-formatted file.
failed=0
for edition in $(printf '%s\n' "$files" | cut -d ' ' -f 1 | sort -u); do
    printf '%s\n' "$files" | sed -n "s/^$edition //p" | tr '\n' '\0' |
        xargs -0 -n 100 rustfmt --check --edition "$edition" || failed=1
done
exit "$failed"
