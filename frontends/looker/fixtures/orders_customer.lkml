view: orders {
  sql_table_name: `dw`.`sales`.`orders` ;;

  dimension: order_id {
    type: number
    primary_key: yes
    sql: ${TABLE}.order_id ;;
  }

  dimension: customer_id {
    type: number
    sql: ${TABLE}.customer_id ;;
  }

  dimension: sale_amount {
    type: number
    sql: ${TABLE}.sale_amount ;;
  }

  measure: revenue {
    type: sum
    sql: ${TABLE}.sale_amount ;;
  }
}

view: customer {
  sql_table_name: `dw`.`sales`.`customer` ;;

  dimension: customer_id {
    type: number
    primary_key: yes
    sql: ${TABLE}.customer_id ;;
  }

  dimension: region_code {
    type: string
    sql: ${TABLE}.region_code ;;
  }
}

explore: orders_customer {
  view_name: orders

  join: customer {
    type: left_outer
    relationship: many_to_one
    sql_on: ${orders.customer_id} = ${customer.customer_id} ;;
  }
}
