package hypercards

import (
	"errors"
	"strings"
	"unicode"
)

var errBadUserInput = errors.New("bad user string")

// ntui disallows RabbitMQ special chars (. # *) from roomID/userID
func ntui(s string) error {
	if false ||
		strings.Contains(s, ".") ||
		strings.Contains(s, "/") ||
		strings.Contains(s, "*") ||
		strings.Contains(s, ">") ||
		false {
		return errBadUserInput
	}
	for _, c := range s {
		if unicode.IsSpace(c) {
			return errBadUserInput
		}
	}
	return nil
}
