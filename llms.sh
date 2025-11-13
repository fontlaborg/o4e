#!/usr/bin/env bash

cd "$(dirname "$0")"

llms . "*.txt,AGENTS.md,CLAUDE.md,GEMINI.md,LLXPRT.md,QWEN.md,WORK.md,issues,target,external,haforu"
