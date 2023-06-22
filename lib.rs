#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod erc20 {
    use ink::storage::Mapping;

    #[ink(storage)]
    #[derive(Default)]
    pub struct Erc20 {
        total_supply: Balance,
        balances: Mapping<AccountId, Balance>,
        allowances: Mapping<(AccountId, AccountId), Balance>,
    }

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        BalanceTooLow,
        AllowanceTooLow,
    }

    type Result<T> = core::result::Result<T, Error>;

    #[ink(event)]
    pub struct Transfer {
        #[ink(topic)]
        from: Option<AccountId>,
        #[ink(topic)]
        to: Option<AccountId>,
        #[ink(topic)]
        value: Balance,
    }

    //approval event
    #[ink(event)]
    pub struct Approval {
        #[ink(topic)]
        owner: AccountId,
        #[ink(topic)]
        spender: AccountId,
        #[ink(topic)]
        value: Balance,
    }

    impl Erc20 {
        #[ink(constructor)]
        pub fn new(total_supply: Balance) -> Self {
            let mut balances = Mapping::new();
            balances.insert(Self::env().caller(), &total_supply);
            Self::env().emit_event(Transfer {
                from: None,
                to: Some(Self::env().caller()),
                value: total_supply,
            });
            Self {
                total_supply,
                balances,
                allowances: Default::default(),
            }
        }

        #[ink(message)]
        pub fn total_supply(&self) -> Balance {
            self.total_supply
        }

        #[ink(message)]
        pub fn balance_of(&self, who: AccountId) -> Balance {
            self.balances.get(&who).unwrap_or_default()
        }

        fn transfer_from_to(
            &mut self,
            from: AccountId,
            to: AccountId,
            value: Balance,
        ) -> Result<()> {
            let balance_from = self.balance_of(from);
            let balance_to = self.balance_of(to);

            if value > balance_from {
                return Err(Error::BalanceTooLow);
            }

            self.balances.insert(from, &(balance_from - value));
            self.balances.insert(to, &(balance_to + value));

            self.env().emit_event(Transfer {
                from: Some(from),
                to: Some(to),
                value,
            });
            Ok(())
        }

        #[ink(message)]
        pub fn transfer(&mut self, to: AccountId, value: Balance) -> Result<()> {
            let from = self.env().caller();
            self.transfer_from_to(from, to, value)
        }

        #[ink(message)]
        pub fn transfer_from(
            &mut self,
            from: AccountId,
            to: AccountId,
            value: Balance,
        ) -> Result<()> {
            let allowance = self
                .allowances
                .get((from, self.env().caller()))
                .unwrap_or_default();
            if value > allowance {
                return Err(Error::AllowanceTooLow);
            }

            self.allowances
                .insert((from, self.env().caller()), &(allowance - value));
            self.transfer_from_to(from, to, value)
        }

        // approve
        #[ink(message)]
        pub fn approve(&mut self, spender: AccountId, value: Balance) -> Result<()> {
            let owner = self.env().caller();
            self.allowances.insert((owner, spender), &value);
            self.env().emit_event(Approval {
                owner,
                spender,
                value,
            });
            Ok(())
        }
    }

    type Event = <Erc20 as ::ink::reflect::ContractEventBase>::Type;
    #[cfg(test)]
    mod tests {
        use super::*;

        #[ink::test]
        fn constructor_works() {
            let erc20 = Erc20::new(123);
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            assert_eq!(erc20.total_supply, 123);
            assert_eq!(erc20.balance_of(accounts.alice), 123);

            let emitted_events = ink::env::test::recorded_events().collect::<Vec<_>>();
            let first_event = emitted_events.get(0).expect("No event found");

            let decoded = <Event as scale::Decode>::decode(&mut &first_event.data[..])
                .expect("Could not decode event data");
            match decoded {
                Event::Transfer(Transfer { from, to, value }) => {
                    assert_eq!(from, None);
                    assert_eq!(to, Some(accounts.alice));
                    assert_eq!(value, 123);
                }
                _ => panic!("No transfer event emitted!"),
            }
        }

        // test transfer
        #[ink::test]
        fn transfer_works() {
            let mut erc20 = Erc20::new(1000);
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            assert_eq!(erc20.balance_of(accounts.alice), 1000);
            assert_eq!(erc20.balance_of(accounts.bob), 0);

            assert_eq!(erc20.transfer(accounts.bob, 100), Ok(()));
            assert_eq!(erc20.balance_of(accounts.alice), 900);
            assert_eq!(erc20.balance_of(accounts.bob), 100);
        }

        // test transfer with low balance
        #[ink::test]
        fn transfer_with_low_balance_show_fail() {
            let mut erc20 = Erc20::new(1000);
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            assert_eq!(erc20.balance_of(accounts.alice), 1000);
            assert_eq!(erc20.balance_of(accounts.bob), 0);

            assert_eq!(
                erc20.transfer(accounts.bob, 1001),
                Err(Error::BalanceTooLow)
            );
            assert_eq!(erc20.balance_of(accounts.alice), 1000);
            assert_eq!(erc20.balance_of(accounts.bob), 0);
        }
    }

    #[cfg(all(test, feature = "e2e-tests"))]
    mod e2e_tests {
        use super::*;
        use ink_e2e::build_message;
        type E2EResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

        // test transfer
        #[ink_e2e::test]
        async fn e2e_transfer(mut client: ink_e2e::Client<C, E>) -> E2EResult<()> {
            // Given
            let constructor = Erc20Ref::new(1000);
            let contract_account_id = client
                .instantiate("erc20", &ink_e2e::alice(), constructor, 0, None)
                .await
                .expect("instantiate failed")
                .account_id;

            let alice_acc = ink_e2e::account_id(ink_e2e::AccountKeyring::Alice);
            let bob_acc = ink_e2e::account_id(ink_e2e::AccountKeyring::Bob);

            // test transfer
            let transfer_msg = build_message::<Erc20Ref>(contract_account_id.clone())
                .call(|erc20| erc20.transfer(bob_acc.clone(), 100));
            let transfer_result = client.call(&ink_e2e::alice(), transfer_msg, 0, None).await;
            assert!(transfer_result.is_ok());

            // check status after transfer
            let balance_of_msg = build_message::<Erc20Ref>(contract_account_id.clone())
                .call(|erc20| erc20.balance_of(alice_acc.clone()));
            let balance_of_result = client
                .call_dry_run(&ink_e2e::alice(), &balance_of_msg, 0, None)
                .await;

            assert_eq!(balance_of_result.return_value(), 900);
            Ok(())
        }
    }
}
