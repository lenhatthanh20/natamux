/* A fixed size ring of order ids. */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>

#define RING_SIZE 8u

typedef struct {
    uint32_t items[RING_SIZE];
    size_t head;
    size_t len;
} Ring;

static bool ring_push(Ring *ring, uint32_t id) {
    if (ring->len == RING_SIZE) {
        return false;
    }
    ring->items[(ring->head + ring->len) % RING_SIZE] = id;
    ring->len++;
    return true;
}

static int ring_pop(Ring *ring, uint32_t *out) {
    if (ring->len == 0) return -1;
    *out = ring->items[ring->head];
    ring->head = (ring->head + 1) % RING_SIZE;
    ring->len -= 1;
    return 0;
}

int main(void) {
    Ring ring = {0};
    for (uint32_t i = 1; i <= 10; ++i) {
        if (!ring_push(&ring, i * 100)) printf("full at %u\n", i);
    }
    uint32_t id;
    while (ring_pop(&ring, &id) == 0) printf("order %u\n", id);
    return 0;
}
