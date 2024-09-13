use std::collections::VecDeque;

use anyhow::{anyhow, Result};
use bitvec::{slice::BitSlice, vec::BitVec};

pub mod nonogrammed;
pub mod webpbn;

pub struct PopulatedBoard {
    pub rows: Vec<Vec<u8>>,
    pub columns: Vec<Vec<u8>>,
    pub solution: BitVec,
}

pub fn populate_board(solution: &BitSlice, rows: u16, columns: u16) -> Result<PopulatedBoard> {
    let rows = rows as usize;
    let columns = columns as usize;
    if solution.len() != rows * columns {
        return Err(anyhow!("Invalid board size."));
    }
    let mut vec_rows: VecDeque<Vec<u8>> = VecDeque::from(vec![vec![0]; rows]);
    let mut vec_columns: VecDeque<Vec<u8>> = VecDeque::from(vec![vec![0]; columns]);
    for row in 0..rows {
        for column in 0..columns {
            if solution[row * columns + column] {
                let last_row = vec_rows[row].last_mut().unwrap();
                *last_row += 1;
                let last_column = vec_columns[column].last_mut().unwrap();
                *last_column += 1;
            } else {
                vec_rows[row].push(0);
                vec_columns[column].push(0);
            }
        }
    }
    vec_rows.iter_mut().for_each(|row| row.retain(|&x| x > 0));
    vec_columns
        .iter_mut()
        .for_each(|column| column.retain(|&x| x > 0));
    let solution = BitVec::from_bitslice(solution);
    Ok(PopulatedBoard {
        rows: vec_rows.into(),
        columns: vec_columns.into(),
        solution,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitvec::{bitvec, order::Lsb0};

    #[test]
    fn it_creates_a_valid_board() {
        let rows = 10;
        let columns = 10;
        let solution = bitvec![
            0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            1, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 0, 0,
            0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0,
            1, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0
        ];
        let board = populate_board(&solution, rows, columns);
        assert!(board.is_ok());
        let board = board.unwrap();
        assert_eq!(
            board.rows,
            vec![
                vec![4],
                vec![1, 3],
                vec![10],
                vec![1, 1, 1, 1],
                vec![1, 2, 1],
                vec![1, 2, 1],
                vec![1, 1, 1, 1],
                vec![1, 2, 1],
                vec![1, 1],
                vec![8]
            ]
        );
        assert_eq!(
            board.columns,
            vec![
                vec![1, 2],
                vec![2, 2, 1],
                vec![2, 2],
                vec![1, 2, 1, 1],
                vec![1, 1, 2, 1, 1],
                vec![3, 2, 1, 1],
                vec![4, 1, 1],
                vec![2, 2],
                vec![2, 2, 1],
                vec![1, 2],
            ]
        );
        assert_eq!(board.solution, solution);
    }

    #[test]
    fn it_creates_rectangular_board() {
        let rows = 5;
        let columns = 8;
        let solution = bitvec![
            1, 1, 1, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0,
            0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1
        ];
        let board = populate_board(&solution, rows, columns);
        assert!(board.is_ok());
        let board = board.unwrap();
        assert_eq!(
            board.rows,
            vec![
                vec![4, 1],
                vec![1, 1],
                vec![1, 2, 1],
                vec![1, 2],
                vec![1, 2, 2],
            ]
        );
        assert_eq!(
            board.columns,
            vec![
                vec![1, 1],
                vec![4],
                vec![1, 1],
                vec![1, 1, 1],
                vec![1],
                vec![],
                vec![4],
                vec![1, 2]
            ]
        );
        assert_eq!(board.solution, solution);
    }

    #[test]
    fn it_does_not_trim_space_around_the_board() {
        let rows = 5;
        let columns = 7;
        let solution = bitvec![
            0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0,
            0, 0, 0, 0, 0, 0
        ];
        let board = populate_board(&solution, rows, columns);
        assert!(board.is_ok());
        let board = board.unwrap();
        assert_eq!(
            board.rows,
            vec![vec![], vec![2], vec![], vec![2, 1], vec![]]
        );
        assert_eq!(
            board.columns,
            vec![
                vec![],
                vec![1, 1],
                vec![1, 1],
                vec![],
                vec![1],
                vec![],
                vec![]
            ]
        );
        assert_eq!(board.solution, solution);
    }

    // #[test]
    // fn it_trims_space_around_the_board() {
    //     let rows = 5;
    //     let columns = 7;
    //     let solution = bitvec![
    //         0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0,
    //         0, 0, 0, 0, 0, 0
    //     ];
    //     let board = populate_board(&solution, rows, columns);
    //     assert!(board.is_ok());
    //     let board = board.unwrap();
    //     assert_eq!(board.rows, vec![vec![2], vec![], vec![2, 1]]);
    //     assert_eq!(board.columns, vec![vec![1, 1], vec![1, 1], vec![], vec![1]]);
    //     assert_eq!(board.solution, bitvec![1, 1, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1,]);
    // }
}
