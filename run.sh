#!/bin/bash

mode=$1

if ! [[ "$mode" == "test" || "$mode" == "run" || "$mode" == "all" ]]; then
  echo "Invalid mode: $mode. Valid modes are: test, run, all." | tee logs/err.txt
  exit 1
fi

: > logs/out.txt
: > logs/err.txt

echo "make $@" | tee -a logs/out.txt

make "$@" >> logs/out.txt 2>> logs/err.txt 

# TODO
# exit_code=$?

# if [[ exit_code != 33 ]]; then
#   echo "[Err] exited with code $exit_code" | tee -a logs/out.txt logs/err.txt
# else
#   echo "[Ok] exited with code $exit_code" | tee -a logs/out.txt
# fi
