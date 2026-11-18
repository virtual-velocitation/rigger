# The fixture side of the FIFO witness (tests/common/witness.rs), sourced first by every fixture
# whose processes a witness reads. It opens the witness FIFO's write end on fd 9 and the release
# FIFO's read end on fd 8, so every process the fixture starts after it holds both: one byte goes
# to fd 9 for each process that is up (a holder's from the shell that started it, once it exists),
# and none exits on its own, since each blocks reading fd 8 until the test drops that FIFO's only
# write end. A process the code under test missed is therefore still running when the test reads
# the witness, and the test's release, never a signal, ends it after.
exec 9>"$RIGGER_TEST_WITNESS" 8<"$RIGGER_TEST_RELEASE"

# The commands that write a process's up byte and that block it until the test releases the
# fixture: run in this shell by `up` and `held`, and by each fresh shell `holder` and `idle` start.
witness_up='printf u >&9'
witness_held='read -r unused <&8'

# Writes this process's up byte.
up() {
    eval "$witness_up"
}

# Blocks until the test releases the fixture.
held() {
    eval "$witness_held"
}

# Starts a holder: a process that holds what every fixture process holds and runs until the test
# releases it, run through the command and arguments given, if any (`holder setsid` starts it in a
# session of its own). The starting shell writes the holder's up byte once the fork has returned,
# so the byte stands for a holder that exists, and the caller goes on only after it is written. A
# holder, as an idle child, runs a fresh shell rather than a subshell, so its command line carries
# none of the fixture's arguments: a session's arguments name its spawn, which marks a process as
# still writing for that spawn.
holder() {
    "$@" sh -c "$witness_held" &
    up
}

# Idles until the test releases the fixture. A trapped signal interrupts only the wait, so a TERM
# trap runs at once; each idle child is a fixture process too.
idle() {
    while :; do
        sh -c "$witness_held" &
        wait $!
        [ $? -gt 128 ] || exit 0
    done
}

# Starts a stand-in daemon in a fresh shell, as a session starts a server other sessions use. Its
# channel is the FIFOs `requests` and `answers` in `$RIGGER_TEST_DAEMON`; it is up once a client has
# opened the channel too, answers each request on a line of its own, and answers `hold` with
# nothing, running until the test releases the fixture.
daemon() {
    sh -c "exec <\"\$RIGGER_TEST_DAEMON/requests\" >\"\$RIGGER_TEST_DAEMON/answers\"; $witness_up
        while read -r request; do
            [ \"\$request\" = hold ] && { $witness_held; exit 0; }
            echo \"answer \$request\"
        done" &
}

# Uses the stand-in daemon as a client, as a session uses a server another session started, and
# says on its output what it sees: the answer to `first`; that it waits once it has sent `hold`;
# how that wait ended; and whether one more request was taken. It ignores SIGPIPE, as most runtimes
# do, so a request with no daemon left to read it fails rather than ending this shell.
client() {
    trap '' PIPE
    exec 3>"$RIGGER_TEST_DAEMON/requests" 4<"$RIGGER_TEST_DAEMON/answers"
    echo first >&3
    read -r answer <&4
    echo "client: answered: $answer"
    echo hold >&3
    echo "client: waiting"
    if read -r answer <&4; then
        echo "client: answered: $answer"
    else
        echo "client: the answers ended"
    fi
    if echo again >&3 2>/dev/null; then
        echo "client: a request was taken"
    else
        echo "client: a request failed"
    fi
}
