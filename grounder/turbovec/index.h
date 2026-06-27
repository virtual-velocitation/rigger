/* Hand-written C ABI for the riggervec shim (grounder/turbovec/shim). The index
 * is opaque to C; Go never dereferences it. */
#ifndef RIGGERVEC_INDEX_H
#define RIGGERVEC_INDEX_H

#include <stddef.h>
#include <stdint.h>

typedef struct IdMapIndex IdMapIndex;

IdMapIndex *rv_index_new(size_t dim, size_t bit_width);
int rv_index_add(IdMapIndex *idx, const float *vectors, size_t n, size_t dim,
                 const uint64_t *ids);
int rv_index_search(const IdMapIndex *idx, const float *query, size_t dim,
                    size_t k, uint64_t *out_ids, float *out_scores);
void rv_index_free(IdMapIndex *idx);

#endif
