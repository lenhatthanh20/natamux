// Package ledger has the edge cases of Go colours.
package ledger

import (
	"errors"
	"fmt"
)

type State int

const (
	Open State = iota
	Paid
	Sent
)

type Row[T any] struct {
	ID    int    `json:"id"`
	Value T      `json:"value,omitempty"`
	Note  string `json:"-"`
}

var ErrEmpty = errors.New("empty")

func Sum[T int | float64](rows []Row[T]) (total T, err error) {
	if len(rows) == 0 {
		return total, ErrEmpty
	}
	defer func() { _ = recover() }()
	done := make(chan struct{})
	go func() {
		defer close(done)
		for _, r := range rows {
			total += r.Value
		}
	}()
	select {
	case <-done:
	}
	raw := `raw\n string`
	r := 'x'
	fmt.Printf("%v %q %c %d\n", total, raw, r, 0x1F)
	return
}
