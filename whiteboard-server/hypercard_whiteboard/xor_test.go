package hypercard_whiteboard

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestXOR(t *testing.T) {
	for _, xs := range [][]bool{
		{false, false, false},
		{false, false, true},
		{false, true, false},
		{false, true, true},
		{true, false, false},
		{true, false, true},
		{true, true, false},
		{true, true, true},
	} {
		t.Run(fmt.Sprintf("xor(%v)", xs[:2]), func(t *testing.T) {
			require.Equal(t, xor(xs[0], xs[1]), xorN(xs[:2]...))
		})
		t.Run(fmt.Sprintf("xor3(%v)", xs), func(t *testing.T) {
			require.Equal(t, xor3(xs[0], xs[1], xs[2]), xorN(xs...))
		})
	}
}
