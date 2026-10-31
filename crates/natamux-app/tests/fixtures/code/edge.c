/* Edge cases of C colours. */
#include <stdio.h>
#include "ledger.h"

#define LIMIT 100
#define SQUARE(x) ((x) * (x))
#ifndef DEBUG
#  define LOG(...) ((void)0)
#else
#  define LOG(...) fprintf(stderr, __VA_ARGS__)
#endif

typedef struct row {
    unsigned int id;
    union { long cents; double rate; } value;
    enum { OPEN, PAID } state;
} row_t;

static int (*compare)(const void *, const void *) = NULL;

int total(const row_t *rows, size_t count) {
    int sum = 0;
    for (size_t i = 0; i < count && i < LIMIT; ++i) {
        sum += rows[i].value.cents > 0 ? (int)rows[i].value.cents : 0;
    }
    char c = '\n';
    LOG("sum %d %c %s\n", sum, c, "done");
    goto end;
end:
    return SQUARE(sum) % 0x7fff;
}
