SELECT
       signature_id, signer_id, signer_printed_name, meaning, reason,
       signed_at, signed_at_zone,
       (
         to_char(timezone(signed_at_zone, signed_at), 'YYYY-MM-DD"T"HH24:MI:SS')
         || CASE
              WHEN timezone(signed_at_zone, signed_at)
                   >= timezone('UTC', signed_at)
              THEN '+' ELSE '-'
            END
         || to_char(
              (abs(extract(epoch from (
                 timezone(signed_at_zone, signed_at)
                 - timezone('UTC', signed_at)
               )))::int / 3600),
              'FM00'
            )
         || ':'
         || to_char(
              ((abs(extract(epoch from (
                 timezone(signed_at_zone, signed_at)
                 - timezone('UTC', signed_at)
               )))::int % 3600) / 60),
              'FM00'
            )
       ) AS signed_at_local,
       record_table, record_id, record_version, doc_type,
       record_content_hash, credential_kind, components_used, superseded_by
  FROM esign.signature
 WHERE signature_id = $1
