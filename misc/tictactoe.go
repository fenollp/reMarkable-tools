package main

import (
	"fmt"
	// "math/rand"
	"time"

	"github.com/gorgonia/agogo/game"
	"github.com/gorgonia/agogo/game/mnk"
	"github.com/gorgonia/agogo/mcts"
)

var (
	// X cross
	X = game.Player(game.Black)
	// O nought
	O = game.Player(game.White)
)

func str(p game.Player) string {
	switch p {
	case X:
		return "X"
	case O:
		return "O"
	}
	panic("unreachable")
}

func opponent(p game.Player) game.Player {
	switch p {
	case X:
		return O
	case O:
		return X
	}
	panic("unreachable")
}

type dummyNN struct{}

func (dummyNN) Infer(state game.State) (policy []float32, value float32) {
	policy = make([]float32, 10)
	switch state.MoveNumber() {
	case 0:
		policy[4] = 0.9
		value = 0.5
	case 1:
		policy[0] = 0.1
		value = 0.5
	case 2:
		policy[2] = 0.9
		value = 8 / 9
	case 3:
		policy[6] = 0.1
		value = 8 / 9
	case 4:
		policy[3] = 0.9
		value = 8 / 9
	case 5:
		policy[5] = 0.1
		value = 0.5
	case 6:
		policy[1] = 0.9
		value = 8 / 9
	case 7:
		policy[7] = 0.1
		value = 0
	case 8:
		policy[8] = 0.9
		value = 0
	}
	return
}

func main() {
	g := mnk.TicTacToe()
	conf := mcts.Config{
		PUCT:           1.0,
		M:              3,
		N:              3,
		Timeout:        500 * time.Millisecond,
		PassPreference: mcts.DontPreferPass,
		Budget:         10000,
		DumbPass:       true,
		RandomCount:    0, // this is a deterministic example
	}
	nn := dummyNN{}
	t := mcts.New(g, conf, nn)
	player := X

	{
		moveNum := g.MoveNumber() // first move
		first := game.Single(0)   // 0: top left
		g = g.Apply(game.PlayerMove{player, first}).(*mnk.MNK)
		fmt.Printf("Turn %d (%s)\n%v---\n", moveNum, str(player), g)
		player = opponent(player)
	}

	var ended bool
	var winner game.Player
	for ended, winner = g.Ended(); !ended; ended, winner = g.Ended() {
		moveNum := g.MoveNumber()
		best := t.Search(player)
		g = g.Apply(game.PlayerMove{player, best}).(*mnk.MNK)
		fmt.Printf("Turn %d (%s)\n%v---\n", moveNum, str(player), g)
		// if moveNum == 2 {
		// 	fmt.Println("fullgraph:", t.ToDot())
		// }
		player = opponent(player)
	}

	fmt.Println("WINNER", str(winner))
}
