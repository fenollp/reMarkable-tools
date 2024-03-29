package hypercards

import (
	"context"
	"errors"
	"strings"
	"time"

	"go.uber.org/zap"
	"google.golang.org/grpc/metadata"
)

var baseLog *zap.Logger

const (
	grpcUserID = "x-user"
)

// MustSetupLogging ...
func MustSetupLogging() {
	var err error
	if baseLog == nil {
		baseLog, err = zap.NewDevelopment(
			zap.AddCaller(),
		)
		if err != nil {
			panic(err)
		}
	}
}

type logingContextKey int

const (
	uniqueIDKey logingContextKey = iota // Serves as request ID, actually: the user's ID
)

func ctxUID(ctx context.Context) string { return ctx.Value(uniqueIDKey).(string) }

// NewLogFromCtx ...
func NewLogFromCtx(ctx context.Context) *zap.Logger {
	l := baseLog
	if ctx != nil {
		if uniqueID, ok := ctx.Value(uniqueIDKey).(string); ok {
			l = l.With(zap.String("", uniqueID))
		}
	}
	return l
}

type authOptions struct {
	noDeadline bool
	allowAnons bool
}

func (opts *authOptions) deadline(ctx context.Context, userID string) (
	context.Context,
	func(),
	error,
) {
	cancel := func() {}
	if !opts.noDeadline {
		ctx, cancel = context.WithTimeout(ctx, 1*500*time.Millisecond)
	}
	if userID != "" {
		ctx = context.WithValue(ctx, uniqueIDKey, userID)
	}
	return ctx, cancel, nil
}

type authOption func(*authOptions)

func optNoDeadline() authOption { return func(a *authOptions) { a.noDeadline = true } }
func optAllowAnons() authOption { return func(a *authOptions) { a.allowAnons = true } }

var errForbidden = errors.New("forbidden")

func (srv *Server) prepare(ctx context.Context, fs ...authOption) (context.Context, func(), error) {
	cancel := func() {}
	opts := &authOptions{}
	for _, f := range fs {
		f(opts)
	}

	if opts.allowAnons {
		return ctx, cancel, nil
	}

	md, ok := metadata.FromIncomingContext(ctx)
	if !ok {
		return ctx, cancel, errForbidden
	}
	var v []string
	if v = md.Get(grpcUserID); len(v) != 1 {
		return ctx, cancel, errForbidden
	}
	userID := v[0]
	if userID == "" || userID != strings.TrimSpace(userID) {
		return ctx, cancel, errForbidden
	}
	if err := ntui(userID); err != nil {
		return ctx, cancel, err
	}

	return opts.deadline(ctx, userID)
}
