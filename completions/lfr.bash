_lfr() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local words='snapshot compare report export doctor config gc --store --system --config --help --version'
    case "${COMP_WORDS[1]}" in
        snapshot) words='create list show delete pin --label --source --allow-pinned' ;;
        compare) words='--format text json' ;;
        report) words='--symptom --output' ;;
        export) words='--preview --output' ;;
        config) words='validate' ;;
        gc) words='--dry-run' ;;
    esac
    COMPREPLY=( $(compgen -W "$words" -- "$cur") )
}
complete -F _lfr lfr
