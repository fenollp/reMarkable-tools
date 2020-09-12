package hypercard_whiteboard

import (
	"errors"
	"strings"
)

var errBadUserInput = errors.New("bad user string")

// ntui disallows RabbitMQ special chars (. # *) from roomID/userID
func ntui(s string) error {
	if false ||
		strings.Contains(s, ".") ||
		strings.Contains(s, "#") ||
		strings.Contains(s, "*") ||
		false {
		return errBadUserInput
	}
	return nil
}
