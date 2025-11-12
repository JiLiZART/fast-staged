## Motivation

I'm tired of using `lint-staged` and got wired errors that .git folder is locked.
I'm tired of that `lint-staged` is not fast enough.

So I started to write my own tool to learn how Rust works.

## Solution

This is a simple tool that runs commands on changed files.

## Features

Uses native rust implementation of git status to get list of changed files.
Uses parallel execution of commands.
Uses pattern matching to match files to commands.
Toml config file.
