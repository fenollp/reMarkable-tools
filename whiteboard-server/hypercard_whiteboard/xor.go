package hypercard_whiteboard

func xor(a, b bool) bool     { return (a && !b) || (!a && b) }
func xor3(a, b, c bool) bool { return (a && !b && !c) || (!a && b && !c) || (!a && !b && c) }

func xorN(xs ...bool) bool {
	z := false
	for _, x := range xs {
		if x && z {
			return false
		}
		if x {
			z = true
		}
	}
	return z
}
