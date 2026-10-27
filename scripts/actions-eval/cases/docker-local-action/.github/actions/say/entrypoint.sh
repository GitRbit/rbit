echo "inside saying $1 (INPUT_WORD=$INPUT_WORD) in $(pwd)"
echo "said=$1!" >> "$GITHUB_OUTPUT"
test -f /github/workspace/.github/actions/say/action.yml
