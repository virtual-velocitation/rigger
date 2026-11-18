# The fixture side of the FIFO witness (tests/common/witness.rs), sourced first by every fixture
# whose processes a witness reads. It opens the witness FIFO's write end on fd 9 and the release
# FIFO's read end on fd 8, so every process the fixture starts after it holds both: one byte goes
# to fd 9 for each process that is up (a holder's from the shell that started it, once it exists),
# and none exits on its own, since each blocks reading fd 8 until the test drops that FIFO's only
# write end. A process the code under test missed is therefore still running when the test reads
# the witness, and the test's release, never a signal, ends it after.
exec 9>"$RIGGER_TEST_WITNESS" 8<"$RIGGER_TEST_RELEASE"

# Writes this process's up byte.
up() {
    printf u >&9
}

# Blocks until the test releases the fixture.
held() {
    read -r unused <&8
}

# Starts a holder: a process that holds what every fixture process holds and runs until the test
# releases it. The starting shell writes the holder's up byte once the fork has returned, so the
# byte stands for a holder that exists, and the caller goes on only after it is written.
holder() {
    held &
    up
}

# Idles until the test releases the fixture. A trapped signal interrupts only the wait, so a TERM
# trap runs at once; each idle child is a fixture process too.
idle() {
    while :; do
        (held) &
        wait $!
        [ $? -gt 128 ] || exit 0
    done
}
