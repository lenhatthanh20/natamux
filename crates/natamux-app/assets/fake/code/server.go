// Package shop serves the cart over HTTP.
package shop

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"sync"
)

var ErrEmpty = errors.New("the cart is empty")

type Item struct {
	Name  string  `json:"name"`
	Price float64 `json:"price"`
}

type Cart struct {
	mu    sync.Mutex
	items []Item
}

func (c *Cart) Total() (float64, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if len(c.items) == 0 {
		return 0, ErrEmpty
	}
	sum := 0.0
	for _, item := range c.items {
		sum += item.Price
	}
	return sum, nil
}

func (c *Cart) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	total, err := c.Total()
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]string{"total": fmt.Sprintf("%.2f", total)})
}
