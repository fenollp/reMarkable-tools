package hypercard_whiteboard

import (
	"errors"
	"fmt"
	"strings"

	"go.uber.org/zap"
)

const (
	ourCurrentMessagingPrefix = "hc.wb.1" // Bump this when breaking RK backward compatibility

	rkEvent = "evt"
)

var (
	_ rabbiter = rkOfEvent{}

	errBadRK = errors.New("bad RK")

	rkEventSample = rkOfEvent{userID: "user", roomID: "room", kind: "*"}

	rkEventLen = len(rkEventSample.encodeBK())

	_ = verifyRabbiter(rkEventSample)
)

type rabbiter interface {
	fmt.Stringer
	encodeBK() []string
}

func fromRK(rk string) interface{} {
	if !strings.HasPrefix(rk, ourCurrentMessagingPrefix) {
		return errBadRK
	}
	parts := strings.Split(strings.TrimPrefix(rk, ourCurrentMessagingPrefix), ".")
	n := len(parts)
	if n == 0 {
		return errBadRK
	}
	n--
	parts = parts[1:]
	switch {
	case n == rkEventLen && parts[0] == rkEvent:
		o := rkOfEvent{}
		o.roomID = parts[1]
		o.userID = parts[2]
		o.kind = parts[3]
		return o

	default:
		return errBadRK
	}
}

func encodeUint64BK(v uint64, vv string) string {
	if vv == "" {
		vv = fmt.Sprintf("%d", v)
	}
	return vv
}
func encodeInt32BK(v int32, vv string) string {
	if vv == "" {
		vv = fmt.Sprintf("%d", v)
	}
	return vv
}
func encodeBK(fields []string) string {
	var b strings.Builder
	b.WriteString(ourCurrentMessagingPrefix)
	for _, field := range fields {
		b.WriteString(".")
		b.WriteString(field)
		if field == "" || field == "0" {
			baseLog.Warn("empty field in RK",
				zap.String("field", field),
				zap.Strings("fields", fields),
			)
		}
	}
	return b.String()
}

func verifyRabbiter(o rabbiter) int {
	fields := o.encodeBK()
	for i, field := range fields {
		if field == "" || field == "0" {
			panic(fmt.Sprintf("empty routing key %+v[%d]", fields, i))
		}
	}
	encoded := encodeBK(fields)
	switch x := fromRK(encoded).(type) {
	case error:
		panic(fmt.Errorf("failed to encodeBK(%+v): %v", fields, x))
	case rabbiter:
		refields := x.encodeBK()
		if strings.Join(refields, ".") != strings.Join(fields, ".") {
			panic(fmt.Errorf("failed to fromRK(encodeBK(%+v)).encodeBK(): %+v", fields, refields))
		}
		if reencoded := encodeBK(refields); reencoded != encoded {
			panic(fmt.Errorf("failed to encodeBK(fromRK(encodeBK(%+v)).encodeBK()): %q", fields, reencoded))
		}
	}
	return len(fields)
}

type rkOfEvent struct {
	roomID string
	userID string
	kind   string
}

func (o rkOfEvent) String() string { return encodeBK(o.encodeBK()) }
func (o rkOfEvent) encodeBK() []string {
	return []string{rkEvent, o.roomID, o.userID, o.kind}
}

// NOTE: MUST be unique + MUST be exhaustive from Event.GetEvent*()
const (
	evtKindDrawing           = "drawing"
	evtKindUserJoinedTheRoom = "userjoinedroom"
	evtKindUserLeftTheRoom   = "userleftroom"
)

func (x *Event) rk() string {
	var kind string
	switch {
	case x.GetEventDrawing() != nil:
		kind = evtKindDrawing
	case x.GetEventUserLeftTheRoom() != false:
		kind = evtKindUserLeftTheRoom
	case x.GetEventUserJoinedTheRoom() != false:
		kind = evtKindUserJoinedTheRoom
	default:
		panic(fmt.Sprintf("unhandled event kind for %+v", x))
	}
	return rkOfEvent{
		roomID: x.RoomId,
		userID: x.UserId,
		kind:   kind,
	}.String()
}
