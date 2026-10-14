#!/bin/sh
set -eu

project_root=$(git rev-parse --show-toplevel)
git config --local core.hooksPath scripts/git_hooks
git config --local commit.template "$project_root/docs/commit_template.txt"
git config --local i18n.commitEncoding UTF-8
git config --local i18n.logOutputEncoding UTF-8
git config --local core.autocrlf false
git config --local core.quotePath false
git config --local pull.ff only
git config --local fetch.prune true

printf '%s\n' '已配置 Try 的 Git hooks、中文提交模板和 UTF-8 编码。'
