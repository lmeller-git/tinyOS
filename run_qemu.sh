#!/bin/sh

echo "\nrunning QEMU\n"

"$@"
EC=$?
echo "\nQEMU exited with code $EC\n"

if [ "$EC" -eq 33 ]; then
    echo "Treating exit code 33 as success\n"
    exit 0
else
    echo "Failure (exit $EC)\n"
    exit "$EC"
fi
